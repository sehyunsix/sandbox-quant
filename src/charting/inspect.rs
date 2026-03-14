use chrono::{DateTime, Utc};

use crate::charting::scene::{
    BarSeries, CandleSeries, ChartScene, Crosshair, EpochMs, HoverModel, LineSeries, MarkerSeries,
    Pane, Series, TooltipModel, TooltipRow, TooltipSection, ValueFormatter,
};

const OUTER_MARGIN: f32 = 12.0;
const LEFT_AXIS_WIDTH: f32 = 72.0;
const RIGHT_PADDING: f32 = 12.0;
const CAPTION_HEIGHT: f32 = 30.0;
const TOP_PADDING: f32 = 8.0;
const X_LABEL_HEIGHT: f32 = 44.0;
const COMPACT_BOTTOM_PADDING: f32 = 12.0;

pub fn hover_model_at(
    scene: &ChartScene,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
) -> Option<HoverModel> {
    let pane = pane_at(scene, y, height)?;
    let plot_rect = pane_plot_rect(scene, pane, width, height);
    if x < plot_rect.left || x > plot_rect.right || y < plot_rect.top || y > plot_rect.bottom {
        return None;
    }

    let (min_x, max_x) = visible_time_bounds(scene)?;
    let local_x =
        ((x - plot_rect.left) / (plot_rect.right - plot_rect.left).max(1.0)).clamp(0.0, 1.0);
    let interpolated_time = interpolate_time(min_x, max_x, local_x);
    let time_ms =
        nearest_visible_time(pane, min_x, max_x, interpolated_time).unwrap_or(interpolated_time);
    let (min_y, max_y) = pane_value_bounds(pane)?;
    let local_y =
        ((y - plot_rect.top) / (plot_rect.bottom - plot_rect.top).max(1.0)).clamp(0.0, 1.0);
    let value = max_y - (max_y - min_y) * f64::from(local_y);
    Some(HoverModel {
        crosshair: Some(Crosshair {
            time_ms,
            value: Some(value),
            color: None,
        }),
        tooltip: Some(tooltip_for_time(scene, pane, time_ms)),
    })
}

pub fn zoom_scene(scene: &mut ChartScene, anchor_ratio: f32, zoom_delta: f32) {
    let Some((full_min, full_max)) = scene_time_bounds(scene) else {
        return;
    };
    let (current_min, current_max) = visible_time_bounds(scene).unwrap_or((full_min, full_max));
    let full_span = (full_max.as_i64() - full_min.as_i64()).max(1);
    let current_span = (current_max.as_i64() - current_min.as_i64()).max(1);
    let factor = 0.85_f64.powf(f64::from(zoom_delta));
    // When the full dataset spans less than one second, keep the clamp bounds ordered.
    let min_span = full_span.clamp(1, 1_000);
    let new_span = ((current_span as f64) * factor)
        .round()
        .clamp(min_span as f64, full_span as f64) as i64;
    let anchor =
        current_min.as_i64() + ((current_span as f32) * anchor_ratio.clamp(0.0, 1.0)) as i64;
    let left_ratio = f64::from(anchor_ratio.clamp(0.0, 1.0));
    let mut new_min = anchor - (new_span as f64 * left_ratio).round() as i64;
    let mut new_max = new_min + new_span;
    if new_min < full_min.as_i64() {
        let shift = full_min.as_i64() - new_min;
        new_min += shift;
        new_max += shift;
    }
    if new_max > full_max.as_i64() {
        let shift = new_max - full_max.as_i64();
        new_min -= shift;
        new_max -= shift;
    }
    scene.viewport.x_range = Some((
        EpochMs::new(new_min),
        EpochMs::new(new_max.max(new_min + 1)),
    ));
}

pub fn pan_scene(scene: &mut ChartScene, delta_ratio: f32) {
    let Some((full_min, full_max)) = scene_time_bounds(scene) else {
        return;
    };
    let (current_min, current_max) = visible_time_bounds(scene).unwrap_or((full_min, full_max));
    let span = (current_max.as_i64() - current_min.as_i64()).max(1);
    let shift = ((span as f32) * delta_ratio) as i64;
    if shift == 0 {
        return;
    }
    let mut new_min = current_min.as_i64() + shift;
    let mut new_max = current_max.as_i64() + shift;
    if new_min < full_min.as_i64() {
        let adjust = full_min.as_i64() - new_min;
        new_min += adjust;
        new_max += adjust;
    }
    if new_max > full_max.as_i64() {
        let adjust = new_max - full_max.as_i64();
        new_min -= adjust;
        new_max -= adjust;
    }
    scene.viewport.x_range = Some((
        EpochMs::new(new_min),
        EpochMs::new(new_max.max(new_min + 1)),
    ));
}

pub fn tooltip_for_time(scene: &ChartScene, pane: &Pane, time_ms: EpochMs) -> TooltipModel {
    let mut sections = Vec::new();
    for series in &pane.series {
        match series {
            Series::Candles(series) => {
                append_candle_tooltip(sections.as_mut(), series, pane, time_ms)
            }
            Series::Bars(series) => append_bar_tooltip(sections.as_mut(), series, pane, time_ms),
            Series::Line(series) => append_line_tooltip(sections.as_mut(), series, pane, time_ms),
            Series::Markers(series) => append_marker_tooltip(sections.as_mut(), series, time_ms),
        }
    }
    TooltipModel {
        title: format_time(time_ms, &scene.time_label_format),
        sections,
    }
}

pub fn format_value(value: f64, formatter: &ValueFormatter) -> String {
    match formatter {
        ValueFormatter::Number {
            decimals,
            prefix,
            suffix,
        } => format!(
            "{prefix}{value:.prec$}{suffix}",
            prec = usize::from(*decimals)
        ),
        ValueFormatter::Compact {
            decimals,
            prefix,
            suffix,
        } => {
            let abs = value.abs();
            let (scaled, unit) = if abs >= 1_000_000_000.0 {
                (value / 1_000_000_000.0, "B")
            } else if abs >= 1_000_000.0 {
                (value / 1_000_000.0, "M")
            } else if abs >= 1_000.0 {
                (value / 1_000.0, "K")
            } else {
                (value, "")
            };
            format!(
                "{prefix}{scaled:.prec$}{unit}{suffix}",
                prec = usize::from(*decimals)
            )
        }
        ValueFormatter::Percent { decimals } => {
            format!("{:.prec$}%", value * 100.0, prec = usize::from(*decimals))
        }
    }
}

pub fn pane_value_bounds(pane: &Pane) -> Option<(f64, f64)> {
    let mut values = pane_points(pane)
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    if pane.y_axis.include_zero {
        values.push(0.0);
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).abs();
    let padding = if span < f64::EPSILON {
        1.0
    } else {
        span * 0.08
    };
    Some((min - padding, max + padding))
}

pub fn scene_time_bounds(scene: &ChartScene) -> Option<(EpochMs, EpochMs)> {
    let mut times = scene
        .panes
        .iter()
        .flat_map(|pane| pane_points(pane).map(|(time, _)| time))
        .collect::<Vec<_>>();
    if times.is_empty() {
        return None;
    }
    times.sort();
    let min = *times.first()?;
    let max = *times.last()?;
    Some(if min == max {
        (min, EpochMs::new(min.as_i64().saturating_add(1)))
    } else {
        (min, max)
    })
}

pub fn visible_time_bounds(scene: &ChartScene) -> Option<(EpochMs, EpochMs)> {
    match (scene.viewport.x_range, scene_time_bounds(scene)) {
        (Some((min, max)), Some((full_min, full_max))) => {
            let clamped_min = EpochMs::new(min.as_i64().max(full_min.as_i64()));
            let clamped_max = EpochMs::new(
                max.as_i64()
                    .min(full_max.as_i64())
                    .max(clamped_min.as_i64() + 1),
            );
            Some((clamped_min, clamped_max))
        }
        (None, full) => full,
        _ => None,
    }
}

pub fn pane_rect(scene: &ChartScene, pane: &Pane, total_height: f32) -> (f32, f32) {
    let total_weight = scene
        .panes
        .iter()
        .map(|pane| pane.weight.max(1) as f32)
        .sum::<f32>()
        .max(1.0);
    let mut top = 0.0f32;
    for current in &scene.panes {
        let pane_height = total_height * (current.weight.max(1) as f32 / total_weight);
        let bottom = top + pane_height;
        if current.id == pane.id {
            return (top, bottom);
        }
        top = bottom;
    }
    (0.0, total_height)
}

fn pane_plot_rect(
    scene: &ChartScene,
    pane: &Pane,
    total_width: f32,
    total_height: f32,
) -> PlotRect {
    let (pane_top, pane_bottom) = pane_rect(scene, pane, total_height);
    let is_last = scene
        .panes
        .last()
        .is_some_and(|current| current.id == pane.id);
    PlotRect {
        left: OUTER_MARGIN + LEFT_AXIS_WIDTH,
        right: total_width - OUTER_MARGIN - RIGHT_PADDING,
        top: pane_top + OUTER_MARGIN + CAPTION_HEIGHT + TOP_PADDING,
        bottom: pane_bottom
            - OUTER_MARGIN
            - if is_last {
                X_LABEL_HEIGHT
            } else {
                COMPACT_BOTTOM_PADDING
            },
    }
}

fn pane_at(scene: &ChartScene, y: f32, total_height: f32) -> Option<&Pane> {
    scene.panes.iter().find(|pane| {
        let (top, bottom) = pane_rect(scene, pane, total_height);
        y >= top && y <= bottom
    })
}

fn pane_points(pane: &Pane) -> impl Iterator<Item = (EpochMs, f64)> + '_ {
    pane.series.iter().flat_map(|series| match series {
        Series::Candles(series) => series
            .candles
            .iter()
            .flat_map(|candle| {
                [
                    (candle.open_time_ms, candle.high),
                    (candle.close_time_ms, candle.low),
                    (candle.open_time_ms, candle.open),
                    (candle.close_time_ms, candle.close),
                ]
            })
            .collect::<Vec<_>>(),
        Series::Bars(series) => series
            .bars
            .iter()
            .flat_map(|bar| [(bar.open_time_ms, 0.0), (bar.close_time_ms, bar.value)])
            .collect::<Vec<_>>(),
        Series::Line(series) => series
            .points
            .iter()
            .map(|point| (point.time_ms, point.value))
            .collect::<Vec<_>>(),
        Series::Markers(series) => series
            .markers
            .iter()
            .map(|marker| (marker.time_ms, marker.value))
            .collect::<Vec<_>>(),
    })
}

fn nearest_visible_time(
    pane: &Pane,
    min_x: EpochMs,
    max_x: EpochMs,
    target: EpochMs,
) -> Option<EpochMs> {
    pane_points(pane)
        .map(|(time, _)| time)
        .filter(|time| *time >= min_x && *time <= max_x)
        .min_by_key(|time| distance(*time, target))
}

fn append_candle_tooltip(
    sections: &mut Vec<TooltipSection>,
    series: &CandleSeries,
    pane: &Pane,
    time_ms: EpochMs,
) {
    let Some(candle) = series
        .candles
        .iter()
        .min_by_key(|candle| distance(candle.close_time_ms, time_ms))
    else {
        return;
    };
    sections.push(TooltipSection {
        title: "OHLC".to_string(),
        rows: vec![
            TooltipRow {
                label: "Open".to_string(),
                value: format_value(candle.open, &pane.y_axis.formatter),
            },
            TooltipRow {
                label: "High".to_string(),
                value: format_value(candle.high, &pane.y_axis.formatter),
            },
            TooltipRow {
                label: "Low".to_string(),
                value: format_value(candle.low, &pane.y_axis.formatter),
            },
            TooltipRow {
                label: "Close".to_string(),
                value: format_value(candle.close, &pane.y_axis.formatter),
            },
        ],
    });
}

fn append_bar_tooltip(
    sections: &mut Vec<TooltipSection>,
    series: &BarSeries,
    pane: &Pane,
    time_ms: EpochMs,
) {
    let Some(bar) = series
        .bars
        .iter()
        .min_by_key(|bar| distance(bar.close_time_ms, time_ms))
    else {
        return;
    };
    sections.push(TooltipSection {
        title: title_case(&series.name),
        rows: vec![TooltipRow {
            label: "Value".to_string(),
            value: format_value(bar.value, &pane.y_axis.formatter),
        }],
    });
}

fn append_line_tooltip(
    sections: &mut Vec<TooltipSection>,
    series: &LineSeries,
    pane: &Pane,
    time_ms: EpochMs,
) {
    let Some(point) = series
        .points
        .iter()
        .min_by_key(|point| distance(point.time_ms, time_ms))
    else {
        return;
    };
    sections.push(TooltipSection {
        title: title_case(&series.name),
        rows: vec![TooltipRow {
            label: "Value".to_string(),
            value: format_value(point.value, &pane.y_axis.formatter),
        }],
    });
}

fn append_marker_tooltip(
    sections: &mut Vec<TooltipSection>,
    series: &MarkerSeries,
    time_ms: EpochMs,
) {
    let rows = series
        .markers
        .iter()
        .filter(|marker| distance(marker.time_ms, time_ms) <= 60_000_u64)
        .map(|marker| TooltipRow {
            label: "Event".to_string(),
            value: marker.label.clone(),
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }
    sections.push(TooltipSection {
        title: "Signals".to_string(),
        rows,
    });
}

fn interpolate_time(min: EpochMs, max: EpochMs, t: f32) -> EpochMs {
    let min_i = min.as_i64() as f64;
    let span = max.as_i64().saturating_sub(min.as_i64()) as f64;
    EpochMs::new((min_i + span * f64::from(t)).round() as i64)
}

fn format_time(time_ms: EpochMs, fmt: &str) -> String {
    DateTime::<Utc>::from_timestamp_millis(time_ms.as_i64())
        .map(|value| value.format(fmt).to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn distance(left: EpochMs, right: EpochMs) -> u64 {
    left.as_i64().abs_diff(right.as_i64())
}

fn title_case(value: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for ch in value.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            result.push(' ');
            capitalize = true;
        } else if capitalize {
            result.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            result.extend(ch.to_lowercase());
        }
    }
    result
}

#[derive(Debug, Clone, Copy)]
struct PlotRect {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charting::scene::{
        ChartScene, LinePoint, LineSeries, Pane, Series, Viewport, YAxisSpec,
    };
    use crate::charting::style::{ChartTheme, RgbColor};

    #[test]
    fn distance_handles_extreme_epoch_values() {
        let left = EpochMs::new(i64::MIN);
        let right = EpochMs::new(i64::MAX);

        assert_eq!(distance(left, right), u64::MAX);
    }

    #[test]
    fn interpolate_time_saturates_large_spans() {
        let min = EpochMs::new(i64::MIN);
        let max = EpochMs::new(i64::MAX);

        let mid = interpolate_time(min, max, 0.5);

        assert!(mid.as_i64() >= min.as_i64());
        assert!(mid.as_i64() <= max.as_i64());
    }

    #[test]
    fn zoom_scene_handles_subsecond_full_span() {
        let mut scene = ChartScene {
            title: "test".to_string(),
            time_label_format: "%H:%M:%S".to_string(),
            theme: ChartTheme::default(),
            viewport: Viewport::default(),
            hover: None,
            panes: vec![Pane {
                id: "pane".to_string(),
                title: None,
                weight: 1,
                y_axis: YAxisSpec::default(),
                series: vec![Series::Line(LineSeries {
                    name: "line".to_string(),
                    color: RgbColor::new(255, 255, 255),
                    width: 1,
                    points: vec![
                        LinePoint {
                            time_ms: EpochMs::new(0),
                            value: 1.0,
                        },
                        LinePoint {
                            time_ms: EpochMs::new(1),
                            value: 2.0,
                        },
                    ],
                })],
            }],
        };

        zoom_scene(&mut scene, 0.5, 1.0);

        assert!(scene.viewport.x_range.is_some());
    }

    #[test]
    fn nearest_visible_time_snaps_to_closest_point_in_view() {
        let pane = Pane {
            id: "pane".to_string(),
            title: None,
            weight: 1,
            y_axis: YAxisSpec::default(),
            series: vec![Series::Line(LineSeries {
                name: "line".to_string(),
                color: RgbColor::new(255, 255, 255),
                width: 1,
                points: vec![
                    LinePoint {
                        time_ms: EpochMs::new(1_000),
                        value: 1.0,
                    },
                    LinePoint {
                        time_ms: EpochMs::new(2_000),
                        value: 2.0,
                    },
                    LinePoint {
                        time_ms: EpochMs::new(3_000),
                        value: 3.0,
                    },
                ],
            })],
        };

        let snapped = nearest_visible_time(
            &pane,
            EpochMs::new(1_500),
            EpochMs::new(3_000),
            EpochMs::new(2_200),
        )
        .expect("snapped time");

        assert_eq!(snapped.as_i64(), 2_000);
    }
}
