use chrono::{DateTime, Utc};
use plotters::coord::types::RangedCoordf64;
use plotters::coord::Shift;
use plotters::prelude::*;

use crate::charting::inspect::{format_value, visible_time_bounds};
use crate::charting::render::{ChartRenderer, RenderError};
use crate::charting::scene::{
    BarSeries, CandleSeries, ChartScene, EpochMs, LineSeries, MarkerSeries, MarkerShape, Pane,
    RenderRequest, RenderedFrame, Series, YAxisSpec,
};
use crate::charting::style::{ChartTheme, RgbColor};

struct CrosshairOverlay<'a> {
    origin_x: i64,
    time_label_format: &'a str,
    time_ms: EpochMs,
    value: Option<f64>,
    color: Option<RgbColor>,
    show_x_labels: bool,
    theme: ChartTheme,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PlottersRenderer;

impl ChartRenderer for PlottersRenderer {
    fn render(
        &self,
        scene: &ChartScene,
        request: &RenderRequest,
    ) -> Result<RenderedFrame, RenderError> {
        let scale = request.pixel_ratio.max(1.0) * f32::from(request.oversample.max(1));
        let width_px = ((request.width_px as f32) * scale).round().max(1.0) as u32;
        let height_px = ((request.height_px as f32) * scale).round().max(1.0) as u32;
        let mut rgb = vec![0_u8; (width_px as usize) * (height_px as usize) * 3];
        {
            let root =
                BitMapBackend::with_buffer(&mut rgb, (width_px, height_px)).into_drawing_area();
            root.fill(&to_plotters(scene.theme.background))
                .map_err(plotters_err)?;

            if scene.panes.is_empty() {
                draw_empty(&root, height_px, scene.theme, "No chart data")?;
            } else {
                let global_x_bounds = scene
                    .panes
                    .iter()
                    .flat_map(collect_points)
                    .collect::<Vec<_>>();

                if global_x_bounds.is_empty() {
                    draw_empty(&root, height_px, scene.theme, "No chart data")?;
                } else {
                    // Preserve the same x-range across panes while letting each pane choose
                    // its own y-range and relative vertical footprint.
                    let panes = split_panes(&root, &scene.panes);
                    let (min_x, max_x) = visible_time_bounds(scene)
                        .map(|(min, max)| (min.as_i64(), max.as_i64()))
                        .unwrap_or_else(|| x_bounds(&global_x_bounds));
                    for (index, pane) in scene.panes.iter().enumerate() {
                        render_pane(
                            &panes[index],
                            pane,
                            width_px,
                            scene,
                            min_x,
                            max_x,
                            index + 1 == scene.panes.len(),
                        )?;
                    }
                }
            }
        }
        Ok(RenderedFrame {
            width_px,
            height_px,
            rgb,
        })
    }
}

fn render_pane<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    pane: &Pane,
    width_px: u32,
    scene: &ChartScene,
    min_x: i64,
    max_x: i64,
    show_x_labels: bool,
) -> Result<(), RenderError> {
    let points = collect_points(pane).collect::<Vec<_>>();
    if points.is_empty() {
        draw_empty(area, area.dim_in_pixel().1, scene.theme, "No pane data")?;
        return Ok(());
    }
    let origin_x = min_x;
    let max_offset_x = max_x.saturating_sub(origin_x).max(1);
    let min_bar_width_x = ((max_offset_x as f64 / width_px.max(1) as f64) * 4.0).max(1.0);
    let (min_y, max_y) = y_bounds(
        points.iter().map(|(_, value)| *value),
        pane.y_axis.include_zero,
    );
    let mut chart = ChartBuilder::on(area)
        .margin(12)
        .x_label_area_size(if show_x_labels { 44 } else { 12 })
        .y_label_area_size(72)
        .caption(
            pane.title.clone().unwrap_or_else(|| scene.title.clone()),
            ("sans-serif", 20)
                .into_font()
                .color(&to_plotters(scene.theme.text)),
        )
        .build_cartesian_2d(0.0_f64..(max_offset_x as f64), min_y..max_y)
        .map_err(plotters_err)?;

    configure_mesh(
        &mut chart,
        width_px,
        origin_x,
        &scene.time_label_format,
        scene.theme,
        &pane.y_axis,
        show_x_labels,
    )?;
    if show_x_labels {
        draw_time_footer_labels(
            &chart,
            width_px,
            origin_x,
            max_offset_x,
            &scene.time_label_format,
            scene.theme,
        )?;
    }

    let highlight_time = scene
        .hover
        .as_ref()
        .and_then(|hover| hover.crosshair.as_ref().map(|crosshair| crosshair.time_ms));
    for series in &pane.series {
        match series {
            Series::Candles(series) => {
                draw_candles(&mut chart, series, scene.theme, highlight_time, origin_x)?
            }
            Series::Bars(series) => draw_bars(
                &mut chart,
                series,
                highlight_time,
                origin_x,
                min_bar_width_x,
            )?,
            Series::Line(series) => draw_line(&mut chart, series, origin_x)?,
            Series::Markers(series) => draw_markers(&mut chart, series, origin_x)?,
        }
    }
    if let Some(crosshair) = scene
        .hover
        .as_ref()
        .and_then(|hover| hover.crosshair.as_ref())
    {
        draw_crosshair(
            &mut chart,
            pane,
            CrosshairOverlay {
                origin_x,
                time_label_format: &scene.time_label_format,
                time_ms: crosshair.time_ms,
                value: crosshair.value,
                color: crosshair.color,
                show_x_labels,
                theme: scene.theme,
            },
        )?;
    }
    Ok(())
}

fn configure_mesh<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    _width_px: u32,
    origin_x: i64,
    time_label_format: &str,
    theme: ChartTheme,
    y_axis: &YAxisSpec,
    _show_x_labels: bool,
) -> Result<(), RenderError> {
    let mut mesh = chart.configure_mesh();
    let formatter = |value: &f64| {
        format_epoch_ms(
            origin_x.saturating_add(value.round() as i64),
            time_label_format,
        )
    };
    let y_formatter = |value: &f64| format_value(*value, &y_axis.formatter);
    mesh.bold_line_style(to_plotters(theme.grid).mix(0.5))
        .light_line_style(to_plotters(theme.grid).mix(0.2))
        .x_labels(0)
        .y_labels(7)
        .label_style(
            ("sans-serif", 12)
                .into_font()
                .color(&to_plotters(theme.axis)),
        )
        .x_label_formatter(&formatter)
        .y_label_formatter(&y_formatter)
        .axis_style(to_plotters(theme.axis));
    if let Some(label) = &y_axis.label {
        mesh.y_desc(label);
    }
    mesh.draw().map_err(plotters_err)
}

fn draw_candles<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    series: &CandleSeries,
    theme: ChartTheme,
    highlight_time: Option<EpochMs>,
    origin_x: i64,
) -> Result<(), RenderError> {
    let stride = chart_stride(series.candles.len(), 420);
    let up_color = to_plotters(series.up_color.unwrap_or(theme.bull_candle));
    let down_color = to_plotters(series.down_color.unwrap_or(theme.bear_candle));
    chart
        .draw_series(series.candles.iter().step_by(stride).map(|candle| {
            CandleStick::new(
                candle.open_time_ms.as_i64().saturating_sub(origin_x) as f64,
                candle.open,
                candle.high,
                candle.low,
                candle.close,
                up_color.filled(),
                down_color.filled(),
                8_u32,
            )
        }))
        .map_err(plotters_err)?;
    if let Some(candle) = highlight_time.and_then(|time| nearest_candle(&series.candles, time)) {
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [
                    (
                        candle.open_time_ms.as_i64().saturating_sub(origin_x) as f64,
                        candle.low,
                    ),
                    (
                        candle.close_time_ms.as_i64().saturating_sub(origin_x) as f64,
                        candle.high,
                    ),
                ],
                RGBColor(245, 245, 250).stroke_width(2),
            )))
            .map_err(plotters_err)?;
    }
    Ok(())
}

fn draw_line<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    series: &LineSeries,
    origin_x: i64,
) -> Result<(), RenderError> {
    if series.points.is_empty() {
        return Ok(());
    }
    let color = to_plotters(series.color);
    chart
        .draw_series(plotters::series::LineSeries::new(
            series.points.iter().map(|point| {
                (
                    point.time_ms.as_i64().saturating_sub(origin_x) as f64,
                    point.value,
                )
            }),
            color.stroke_width(series.width),
        ))
        .map_err(plotters_err)?;
    Ok(())
}

fn draw_bars<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    series: &BarSeries,
    highlight_time: Option<EpochMs>,
    origin_x: i64,
    min_bar_width_x: f64,
) -> Result<(), RenderError> {
    if series.bars.is_empty() {
        return Ok(());
    }
    let stride = chart_stride(series.bars.len(), 420);
    chart
        .draw_series(series.bars.iter().step_by(stride).map(|bar| {
            let color = to_plotters(bar.color.unwrap_or(series.color));
            let (left_x, right_x) = bar_x_bounds(bar, origin_x, min_bar_width_x);
            Rectangle::new(
                [(left_x, 0.0), (right_x, bar.value)],
                color.mix(0.75).filled(),
            )
        }))
        .map_err(plotters_err)?;
    if let Some(bar) = highlight_time.and_then(|time| nearest_bar(&series.bars, time)) {
        let (left_x, right_x) = bar_x_bounds(bar, origin_x, min_bar_width_x);
        chart
            .draw_series(std::iter::once(Rectangle::new(
                [(left_x, 0.0), (right_x, bar.value)],
                RGBColor(245, 245, 250).stroke_width(2),
            )))
            .map_err(plotters_err)?;
    }
    Ok(())
}

fn draw_markers<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    series: &MarkerSeries,
    origin_x: i64,
) -> Result<(), RenderError> {
    for marker in &series.markers {
        match marker.shape {
            MarkerShape::Circle => {
                chart
                    .draw_series(std::iter::once(Circle::new(
                        (
                            marker.time_ms.as_i64().saturating_sub(origin_x) as f64,
                            marker.value,
                        ),
                        marker.size,
                        to_plotters(marker.color).filled(),
                    )))
                    .map_err(plotters_err)?;
            }
            MarkerShape::Cross => {
                chart
                    .draw_series(std::iter::once(Cross::new(
                        (
                            marker.time_ms.as_i64().saturating_sub(origin_x) as f64,
                            marker.value,
                        ),
                        marker.size,
                        to_plotters(marker.color).stroke_width(2),
                    )))
                    .map_err(plotters_err)?;
            }
        }
    }
    Ok(())
}

fn draw_crosshair<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    pane: &Pane,
    overlay_config: CrosshairOverlay<'_>,
) -> Result<(), RenderError> {
    let x = overlay_config
        .time_ms
        .as_i64()
        .saturating_sub(overlay_config.origin_x) as f64;
    let crosshair_color = to_plotters(overlay_config.color.unwrap_or(RgbColor::new(180, 190, 210)));
    let pane_points = collect_points(pane).collect::<Vec<_>>();
    if pane_points.is_empty() {
        return Ok(());
    }
    let (min_y, max_y) = y_bounds(
        pane_points.iter().map(|(_, pane_value)| *pane_value),
        pane.y_axis.include_zero,
    );
    chart
        .draw_series(std::iter::once(PathElement::new(
            vec![(x, min_y), (x, max_y)],
            crosshair_color.stroke_width(1),
        )))
        .map_err(plotters_err)?;
    let overlay = chart.plotting_area().strip_coord_spec();
    let (x_range, y_range) = chart.plotting_area().get_pixel_range();
    let x_pixel = chart.as_coord_spec().translate(&(x, min_y)).0;
    if let Some(y) = overlay_config.value {
        let (min_x, max_x) = x_bounds(&pane_points);
        let min_x = min_x.saturating_sub(overlay_config.origin_x) as f64;
        let max_x = max_x.saturating_sub(overlay_config.origin_x) as f64;
        chart
            .draw_series(std::iter::once(PathElement::new(
                vec![(min_x, y), (max_x, y)],
                crosshair_color.mix(0.55).stroke_width(1),
            )))
            .map_err(plotters_err)?;
        let y_pixel = chart.as_coord_spec().translate(&(min_x, y)).1;
        let value_text = format_value(y, &pane.y_axis.formatter);
        let label_width = (value_text.len() as i32 * 8).max(52);
        overlay
            .draw(&Rectangle::new(
                [
                    (x_range.end - label_width - 8, y_pixel - 11),
                    (x_range.end - 4, y_pixel + 11),
                ],
                to_plotters(overlay_config.theme.background).filled(),
            ))
            .map_err(plotters_err)?;
        overlay
            .draw(&Rectangle::new(
                [
                    (x_range.end - label_width - 8, y_pixel - 11),
                    (x_range.end - 4, y_pixel + 11),
                ],
                crosshair_color.stroke_width(1),
            ))
            .map_err(plotters_err)?;
        overlay
            .draw(&Text::new(
                value_text,
                (x_range.end - label_width - 2, y_pixel - 8),
                ("sans-serif", 12)
                    .into_font()
                    .color(&to_plotters(overlay_config.theme.text)),
            ))
            .map_err(plotters_err)?;
    }

    if overlay_config.show_x_labels {
        let time_text = format_epoch_ms(
            overlay_config.origin_x.saturating_add(x.round() as i64),
            overlay_config.time_label_format,
        );
        let label_width = (time_text.len() as i32 * 8).max(80);
        let left =
            (x_pixel - label_width / 2).clamp(x_range.start + 4, x_range.end - label_width - 4);
        let top = y_range.end - 24;
        overlay
            .draw(&Rectangle::new(
                [(left, top), (left + label_width, top + 20)],
                to_plotters(overlay_config.theme.background).filled(),
            ))
            .map_err(plotters_err)?;
        overlay
            .draw(&Rectangle::new(
                [(left, top), (left + label_width, top + 20)],
                crosshair_color.stroke_width(1),
            ))
            .map_err(plotters_err)?;
        overlay
            .draw(&Text::new(
                time_text,
                (left + 6, top + 4),
                ("sans-serif", 12)
                    .into_font()
                    .color(&to_plotters(overlay_config.theme.text)),
            ))
            .map_err(plotters_err)?;
    }
    Ok(())
}

fn draw_time_footer_labels<DB: DrawingBackend>(
    chart: &ChartContext<'_, DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    width_px: u32,
    origin_x: i64,
    max_offset_x: i64,
    time_label_format: &str,
    theme: ChartTheme,
) -> Result<(), RenderError> {
    let overlay = chart.plotting_area().strip_coord_spec();
    let (x_range, y_range) = chart.plotting_area().get_pixel_range();
    let label_y = y_range.end - 18;
    let label_count = footer_label_count(width_px);
    let offsets = footer_label_offsets(max_offset_x, label_count);
    let available_width = (x_range.end - x_range.start).max(1);
    let mut last_right = i32::MIN;

    for offset_ms in offsets {
        let ratio = if max_offset_x <= 0 {
            0.0
        } else {
            offset_ms as f64 / max_offset_x as f64
        };
        let x = x_range.start + (ratio * f64::from(available_width)).round() as i32;
        let label = format_epoch_ms(origin_x.saturating_add(offset_ms), time_label_format);
        let label_width = (label.len() as i32 * 7).max(56);
        let left = (x - label_width / 2).clamp(x_range.start + 4, x_range.end - label_width - 4);
        let right = left + label_width;
        if left <= last_right + 8 {
            continue;
        }
        overlay
            .draw(&Text::new(
                label,
                (left, label_y),
                ("sans-serif", 11)
                    .into_font()
                    .color(&to_plotters(theme.axis)),
            ))
            .map_err(plotters_err)?;
        last_right = right;
    }
    Ok(())
}

fn draw_empty<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    height_px: u32,
    theme: ChartTheme,
    message: &str,
) -> Result<(), RenderError> {
    area.draw(&Text::new(
        message,
        (24, (height_px / 2) as i32),
        ("sans-serif", 18)
            .into_font()
            .color(&to_plotters(theme.text)),
    ))
    .map_err(plotters_err)
}

fn collect_points<'a>(pane: &'a Pane) -> impl Iterator<Item = (i64, f64)> + 'a {
    pane.series.iter().flat_map(|series| match series {
        Series::Candles(series) => series
            .candles
            .iter()
            .flat_map(|candle| {
                [
                    (candle.open_time_ms.as_i64(), candle.high),
                    (candle.close_time_ms.as_i64(), candle.low),
                    (candle.open_time_ms.as_i64(), candle.open),
                    (candle.close_time_ms.as_i64(), candle.close),
                ]
            })
            .collect::<Vec<_>>(),
        Series::Bars(series) => series
            .bars
            .iter()
            .flat_map(|bar| {
                [
                    (bar.open_time_ms.as_i64(), 0.0),
                    (bar.close_time_ms.as_i64(), bar.value),
                ]
            })
            .collect::<Vec<_>>(),
        Series::Line(series) => series
            .points
            .iter()
            .map(|point| (point.time_ms.as_i64(), point.value))
            .collect::<Vec<_>>(),
        Series::Markers(series) => series
            .markers
            .iter()
            .map(|marker| (marker.time_ms.as_i64(), marker.value))
            .collect::<Vec<_>>(),
    })
}

fn split_panes<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    panes: &[Pane],
) -> Vec<DrawingArea<DB, Shift>> {
    if panes.len() <= 1 {
        return vec![root.clone()];
    }

    let total_weight = panes
        .iter()
        .map(|pane| pane.weight.max(1) as i32)
        .sum::<i32>()
        .max(1);
    let root_height = root.dim_in_pixel().1 as i32;
    let mut cumulative = 0i32;
    let mut breakpoints = Vec::new();

    for pane in panes.iter().take(panes.len() - 1) {
        cumulative += pane.weight.max(1) as i32;
        breakpoints.push((root_height * cumulative) / total_weight);
    }

    root.split_by_breakpoints::<i32, i32, _, _>([], breakpoints)
}

fn x_bounds(points: &[(i64, f64)]) -> (i64, i64) {
    let min = points
        .iter()
        .map(|(time, _)| *time)
        .min()
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    let max = points
        .iter()
        .map(|(time, _)| *time)
        .max()
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    if min == max {
        (min, max.saturating_add(1_000))
    } else {
        (min, max)
    }
}

fn y_bounds(values: impl Iterator<Item = f64>, include_zero: bool) -> (f64, f64) {
    let values = values.collect::<Vec<_>>();
    let mut min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let mut max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if include_zero {
        min = min.min(0.0);
        max = max.max(0.0);
    }
    let span = (max - min).abs();
    let padding = if span < f64::EPSILON {
        1.0
    } else {
        span * 0.08
    };
    (min - padding, max + padding)
}

fn chart_stride(len: usize, max_points: usize) -> usize {
    if len <= max_points || max_points == 0 {
        1
    } else {
        (len / max_points).max(1)
    }
}

fn bar_x_bounds(
    bar: &crate::charting::scene::Bar,
    origin_x: i64,
    min_bar_width_x: f64,
) -> (f64, f64) {
    let open_x = bar.open_time_ms.as_i64().saturating_sub(origin_x) as f64;
    let close_x = bar.close_time_ms.as_i64().saturating_sub(origin_x) as f64;
    let center = (open_x + close_x) * 0.5;
    let native_width = (close_x - open_x).abs();
    let width = native_width.max(min_bar_width_x);
    (center - width * 0.5, center + width * 0.5)
}

fn footer_label_count(width_px: u32) -> usize {
    (width_px / 220).clamp(3, 6) as usize
}

fn footer_label_offsets(max_offset_x: i64, count: usize) -> Vec<i64> {
    if count <= 1 || max_offset_x <= 0 {
        return vec![0];
    }
    let last = count - 1;
    (0..count)
        .map(|index| ((max_offset_x as i128 * index as i128) / last as i128) as i64)
        .collect()
}

fn format_epoch_ms(ms: i64, fmt: &str) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|value| value.format(fmt).to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn to_plotters(color: RgbColor) -> RGBColor {
    RGBColor(color.r, color.g, color.b)
}

fn plotters_err<E: std::fmt::Display>(error: E) -> RenderError {
    RenderError::new(error.to_string())
}

fn nearest_candle(
    candles: &[crate::charting::scene::Candle],
    time: EpochMs,
) -> Option<&crate::charting::scene::Candle> {
    candles
        .iter()
        .min_by_key(|candle| distance_ms(candle.close_time_ms, time))
}

fn nearest_bar(
    bars: &[crate::charting::scene::Bar],
    time: EpochMs,
) -> Option<&crate::charting::scene::Bar> {
    bars.iter()
        .min_by_key(|bar| distance_ms(bar.close_time_ms, time))
}

fn distance_ms(left: EpochMs, right: EpochMs) -> u64 {
    left.as_i64().abs_diff(right.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_ms_handles_extreme_epoch_values() {
        let left = EpochMs::new(i64::MIN);
        let right = EpochMs::new(i64::MAX);

        assert_eq!(distance_ms(left, right), u64::MAX);
    }

    #[test]
    fn footer_label_offsets_include_start_and_end_without_overflow() {
        let max_offset = 31_667_000_i64;
        let offsets = footer_label_offsets(max_offset, 5);

        assert_eq!(offsets.first().copied(), Some(0));
        assert_eq!(offsets.last().copied(), Some(max_offset));
        assert_eq!(offsets.len(), 5);
        assert!(offsets.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn bar_x_bounds_enforces_minimum_visual_width() {
        let bar = crate::charting::scene::Bar {
            open_time_ms: EpochMs::new(1_000),
            close_time_ms: EpochMs::new(1_001),
            value: 10.0,
            color: None,
        };

        let (left, right) = bar_x_bounds(&bar, 0, 8.0);

        assert!((right - left) >= 8.0);
    }
}
