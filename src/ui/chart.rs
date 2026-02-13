use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use std::char;

use crate::model::candle::{Candle, CandleBuilder};
use crate::model::order::OrderSide;

#[derive(Debug, Clone)]
pub struct FillMarker {
    pub candle_index: usize,
    pub price: f64,
    pub side: OrderSide,
}

pub struct PriceChart<'a> {
    candles: &'a [Candle],
    current_candle: Option<&'a CandleBuilder>,
    fill_markers: &'a [FillMarker],
    fast_sma: Option<f64>,
    slow_sma: Option<f64>,
    fast_sma_period: usize,
    slow_sma_period: usize,
    symbol: &'a str,
}

impl<'a> PriceChart<'a> {
    pub fn new(candles: &'a [Candle], symbol: &'a str) -> Self {
        Self {
            candles,
            current_candle: None,
            fill_markers: &[],
            fast_sma: None,
            slow_sma: None,
            fast_sma_period: 10,
            slow_sma_period: 30,
            symbol,
        }
    }

    pub fn current_candle(mut self, val: Option<&'a CandleBuilder>) -> Self {
        self.current_candle = val;
        self
    }

    pub fn fast_sma(mut self, val: Option<f64>) -> Self {
        self.fast_sma = val;
        self
    }

    pub fn slow_sma(mut self, val: Option<f64>) -> Self {
        self.slow_sma = val;
        self
    }

    pub fn sma_periods(mut self, fast_period: usize, slow_period: usize) -> Self {
        self.fast_sma_period = fast_period.max(1);
        self.slow_sma_period = slow_period.max(1);
        self
    }

    pub fn fill_markers(mut self, val: &'a [FillMarker]) -> Self {
        self.fill_markers = val;
        self
    }
}

fn sma_at(candles: &[&Candle], end_index: usize, period: usize) -> Option<f64> {
    if period == 0 || end_index + 1 < period {
        return None;
    }
    let start = end_index + 1 - period;
    let sum: f64 = candles[start..=end_index].iter().map(|c| c.close).sum();
    Some(sum / period as f64)
}

fn braille_bit(local_x: u8, local_y: u8) -> u8 {
    match (local_x, local_y) {
        (0, 0) => 1 << 0, // dot 1
        (0, 1) => 1 << 1, // dot 2
        (0, 2) => 1 << 2, // dot 3
        (0, 3) => 1 << 6, // dot 7
        (1, 0) => 1 << 3, // dot 4
        (1, 1) => 1 << 4, // dot 5
        (1, 2) => 1 << 5, // dot 6
        (1, 3) => 1 << 7, // dot 8
        _ => 0,
    }
}

fn rasterize_braille_polyline(
    points: &[(i32, i32)],
    cell_width: usize,
    cell_height: usize,
) -> Vec<u8> {
    let mut cells = vec![0_u8; cell_width * cell_height];
    if points.is_empty() || cell_width == 0 || cell_height == 0 {
        return cells;
    }

    let subpixel_w = (cell_width as i32) * 2;
    let subpixel_h = (cell_height as i32) * 4;

    let mut plot = |sx: i32, sy: i32| {
        if sx < 0 || sy < 0 || sx >= subpixel_w || sy >= subpixel_h {
            return;
        }
        let cell_x = (sx / 2) as usize;
        let cell_y = (sy / 4) as usize;
        let local_x = (sx % 2) as u8;
        let local_y = (sy % 4) as u8;
        let idx = cell_y * cell_width + cell_x;
        cells[idx] |= braille_bit(local_x, local_y);
    };

    if points.len() == 1 {
        plot(points[0].0, points[0].1);
        return cells;
    }

    for window in points.windows(2) {
        let (mut x0, mut y0) = window[0];
        let (x1, y1) = window[1];
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            plot(x0, y0);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    cells
}

fn render_braille_cells(
    buf: &mut Buffer,
    cells: &[u8],
    cell_width: usize,
    cell_height: usize,
    x_offset: u16,
    y_offset: u16,
    color: Color,
) {
    for y in 0..cell_height {
        for x in 0..cell_width {
            let mask = cells[y * cell_width + x];
            if mask == 0 {
                continue;
            }
            let ch = char::from_u32(0x2800 + u32::from(mask)).unwrap_or(' ');
            buf.set_string(
                x_offset + x as u16,
                y_offset + y as u16,
                ch.to_string(),
                Style::default().fg(color),
            );
        }
    }
}

impl Widget for PriceChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build visible candle list (finalized + in-progress)
        let in_progress: Option<Candle> = self.current_candle.map(|cb| cb.finish());
        let all_candles: Vec<&Candle> = self.candles.iter().chain(in_progress.as_ref()).collect();

        // Build title with current price + latest volume
        let title = if let Some(c) = all_candles.last() {
            format!(" {} | {:.2} | vol {:.4} ", self.symbol, c.close, c.volume)
        } else {
            format!(" {} | --- ", self.symbol)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        if all_candles.is_empty() || inner.height < 3 || inner.width < 10 {
            if inner.height >= 1 && inner.width >= 10 {
                buf.set_string(
                    inner.x + 1,
                    inner.y,
                    "Waiting for data...",
                    Style::default().fg(Color::DarkGray),
                );
            }
            return;
        }

        // Reserve 10 chars on left for Y-axis labels, 12 on right for current price label
        let left_label_width: u16 = 10;
        let right_label_width: u16 = 12;
        let chart_x_start = inner.x + left_label_width;
        let chart_width = inner
            .width
            .saturating_sub(left_label_width + right_label_width)
            as usize;
        let chart_height = inner.height as usize;

        if chart_width < 2 || chart_height < 2 {
            return;
        }

        // Take the last `chart_width` candles
        let visible_start = all_candles.len().saturating_sub(chart_width);
        let visible: Vec<&Candle> = if all_candles.len() > chart_width {
            all_candles[visible_start..].to_vec()
        } else {
            all_candles.clone()
        };

        // Reserve a small area at the bottom for volume bars.
        let volume_area_height = if chart_height >= 10 { 3 } else { 0 };
        let has_volume_area = volume_area_height > 0 && chart_height > volume_area_height + 1;
        let price_height = if has_volume_area {
            chart_height - volume_area_height - 1
        } else {
            chart_height
        };
        if price_height < 2 {
            return;
        }

        // Find data min/max from visible candle wicks
        let data_min = visible.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let data_max = visible
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let data_range = data_max - data_min;
        let data_range = if data_range < 0.01 { 1.0 } else { data_range };

        // Center Y-axis on current price
        let current_price = visible.last().map(|c| c.close).unwrap_or(data_min);
        let half_range = data_range / 2.0 * 1.1; // 10% padding
        let min_price = current_price - half_range;
        let max_price = current_price + half_range;
        let range = max_price - min_price;

        // Helper: convert price to row index (0 = top, price_height-1 = bottom)
        let price_to_row = |price: f64| -> u16 {
            let normalized = ((price - min_price) / range).clamp(0.0, 1.0);
            let row = ((1.0 - normalized) * (price_height - 1) as f64).round() as u16;
            row.min((price_height - 1) as u16)
        };

        // Draw Y-axis labels (top, middle, bottom)
        let mid_price = (max_price + min_price) / 2.0;
        buf.set_string(
            inner.x,
            inner.y,
            &format!("{:>9.1}", max_price),
            Style::default().fg(Color::DarkGray),
        );
        if price_height > 2 {
            buf.set_string(
                inner.x,
                inner.y + (price_height / 2) as u16,
                &format!("{:>9.1}", mid_price),
                Style::default().fg(Color::DarkGray),
            );
        }
        buf.set_string(
            inner.x,
            inner.y + (price_height - 1) as u16,
            &format!("{:>9.1}", min_price),
            Style::default().fg(Color::DarkGray),
        );

        // Draw each candlestick
        let chart_x_end = chart_x_start + chart_width as u16;
        for (i, candle) in visible.iter().enumerate() {
            let x = chart_x_start + i as u16;
            if x >= chart_x_end {
                break;
            }

            let color = if candle.is_bullish() {
                Color::Green
            } else {
                Color::Red
            };

            let high_row = price_to_row(candle.high);
            let low_row = price_to_row(candle.low);

            let body_top_price = candle.open.max(candle.close);
            let body_bot_price = candle.open.min(candle.close);
            let body_top_row = price_to_row(body_top_price);
            let body_bot_row = price_to_row(body_bot_price);

            // Draw wick (from high to low)
            for row in high_row..=low_row {
                let y = inner.y + row;
                buf.set_string(x, y, "│", Style::default().fg(Color::DarkGray));
            }

            // Draw body (from body_top to body_bot), overwrites wick
            if body_top_row == body_bot_row {
                let y = inner.y + body_top_row;
                buf.set_string(x, y, "─", Style::default().fg(color));
            } else {
                for row in body_top_row..=body_bot_row {
                    let y = inner.y + row;
                    buf.set_string(x, y, "█", Style::default().fg(color));
                }
            }
        }

        // Draw smooth moving average lines via braille subpixels (2x4 per cell).
        let mut fast_points: Vec<(i32, i32)> = Vec::with_capacity(visible.len());
        let mut slow_points: Vec<(i32, i32)> = Vec::with_capacity(visible.len());
        for (i, _) in visible.iter().enumerate() {
            let global_idx = visible_start + i;
            if chart_x_start + i as u16 >= chart_x_end {
                break;
            }
            if let Some(fast) = sma_at(&all_candles, global_idx, self.fast_sma_period) {
                if fast >= min_price && fast <= max_price {
                    let y = price_to_row(fast) as i32;
                    fast_points.push((i as i32 * 2 + 1, y * 4 + 2));
                }
            }
            if let Some(slow) = sma_at(&all_candles, global_idx, self.slow_sma_period) {
                if slow >= min_price && slow <= max_price {
                    let y = price_to_row(slow) as i32;
                    slow_points.push((i as i32 * 2 + 1, y * 4 + 2));
                }
            }
        }
        let fast_cells = rasterize_braille_polyline(&fast_points, chart_width, price_height);
        render_braille_cells(
            buf,
            &fast_cells,
            chart_width,
            price_height,
            chart_x_start,
            inner.y,
            Color::Green,
        );
        let slow_cells = rasterize_braille_polyline(&slow_points, chart_width, price_height);
        render_braille_cells(
            buf,
            &slow_cells,
            chart_width,
            price_height,
            chart_x_start,
            inner.y,
            Color::Yellow,
        );

        // Draw fill markers (BUY/SELL) on top of corresponding candles
        for marker in self.fill_markers {
            if marker.candle_index < visible_start
                || marker.candle_index >= visible_start + visible.len()
            {
                continue;
            }
            if marker.price < min_price || marker.price > max_price {
                continue;
            }

            let x = chart_x_start + (marker.candle_index - visible_start) as u16;
            if x >= chart_x_end {
                continue;
            }
            let y = inner.y + price_to_row(marker.price);
            let (ch, color) = match marker.side {
                OrderSide::Buy => ('B', Color::Green),
                OrderSide::Sell => ('S', Color::Red),
            };
            buf.set_string(
                x,
                y,
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            );
        }

        // Draw current price label on the right side
        let price_row = price_to_row(current_price);
        let right_x = chart_x_end;
        if right_x + right_label_width <= inner.x + inner.width {
            let price_label = format!("▶{:.1}", current_price);
            let price_color = if let Some(c) = visible.last() {
                if c.is_bullish() {
                    Color::Green
                } else {
                    Color::Red
                }
            } else {
                Color::White
            };
            buf.set_string(
                right_x,
                inner.y + price_row,
                &price_label,
                Style::default()
                    .fg(Color::Black)
                    .bg(price_color)
                    .add_modifier(Modifier::BOLD),
            );
        }

        // Draw SMA markers on the rightmost candle column
        let sma_x = chart_x_start + visible.len().min(chart_width).saturating_sub(1) as u16;
        if sma_x < chart_x_end {
            if let Some(fast) = self.fast_sma {
                if fast >= min_price && fast <= max_price {
                    let y = inner.y + price_to_row(fast);
                    buf.set_string(
                        sma_x,
                        y,
                        "F",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            }
            if let Some(slow) = self.slow_sma {
                if slow >= min_price && slow <= max_price {
                    let y = inner.y + price_to_row(slow);
                    buf.set_string(
                        sma_x,
                        y,
                        "S",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                }
            }
        }

        // Draw volume bars in bottom area
        if has_volume_area {
            let separator_y = inner.y + price_height as u16;
            for x in chart_x_start..chart_x_end {
                buf.set_string(x, separator_y, "─", Style::default().fg(Color::DarkGray));
            }
            buf.set_string(
                inner.x + 1,
                separator_y,
                "VOL",
                Style::default().fg(Color::DarkGray),
            );

            let max_volume = visible
                .iter()
                .map(|c| c.volume)
                .fold(0.0_f64, f64::max)
                .max(1e-12);
            let vol_bottom_y = inner.y + (chart_height - 1) as u16;
            for (i, candle) in visible.iter().enumerate() {
                let x = chart_x_start + i as u16;
                if x >= chart_x_end {
                    break;
                }
                let ratio = (candle.volume / max_volume).clamp(0.0, 1.0);
                let bar_height = (ratio * volume_area_height as f64).round() as usize;
                let bar_height = bar_height.max(1);
                let color = if candle.is_bullish() {
                    Color::Green
                } else {
                    Color::Red
                };
                for offset in 0..bar_height.min(volume_area_height) {
                    let y = vol_bottom_y.saturating_sub(offset as u16);
                    buf.set_string(x, y, "█", Style::default().fg(color));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{braille_bit, rasterize_braille_polyline, sma_at};
    use crate::model::candle::Candle;

    #[test]
    fn sma_at_returns_expected_value() {
        let candles = vec![
            Candle {
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 0.0,
                open_time: 0,
                close_time: 1,
            },
            Candle {
                open: 2.0,
                high: 2.0,
                low: 2.0,
                close: 2.0,
                volume: 0.0,
                open_time: 1,
                close_time: 2,
            },
            Candle {
                open: 3.0,
                high: 3.0,
                low: 3.0,
                close: 3.0,
                volume: 0.0,
                open_time: 2,
                close_time: 3,
            },
        ];
        let refs: Vec<&Candle> = candles.iter().collect();
        assert_eq!(sma_at(&refs, 2, 2), Some(2.5));
        assert_eq!(sma_at(&refs, 1, 3), None);
    }

    #[test]
    fn braille_bit_mapping_is_correct() {
        assert_eq!(braille_bit(0, 0), 0b0000_0001);
        assert_eq!(braille_bit(1, 0), 0b0000_1000);
        assert_eq!(braille_bit(0, 3), 0b0100_0000);
        assert_eq!(braille_bit(1, 3), 0b1000_0000);
    }

    #[test]
    fn rasterize_braille_polyline_sets_cells() {
        let points = vec![(0, 0), (3, 7)];
        let cells = rasterize_braille_polyline(&points, 2, 2);
        assert_eq!(cells.len(), 4);
        assert!(cells.iter().any(|m| *m != 0));
    }
}
