use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};

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

    pub fn fill_markers(mut self, val: &'a [FillMarker]) -> Self {
        self.fill_markers = val;
        self
    }
}

impl Widget for PriceChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build visible candle list (finalized + in-progress)
        let in_progress: Option<Candle> = self.current_candle.map(|cb| cb.finish());
        let all_candles: Vec<&Candle> = self
            .candles
            .iter()
            .chain(in_progress.as_ref())
            .collect();

        // Build title with current price
        let title = if let Some(c) = all_candles.last() {
            format!(" {} | {:.2} ", self.symbol, c.close)
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
            .saturating_sub(left_label_width + right_label_width) as usize;
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

        // Find data min/max from visible candle wicks
        let data_min = visible
            .iter()
            .map(|c| c.low)
            .fold(f64::INFINITY, f64::min);
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

        // Helper: convert price to row index (0 = top, chart_height-1 = bottom)
        let price_to_row = |price: f64| -> u16 {
            let normalized = ((price - min_price) / range).clamp(0.0, 1.0);
            let row = ((1.0 - normalized) * (chart_height - 1) as f64).round() as u16;
            row.min(inner.height - 1)
        };

        // Draw Y-axis labels (top, middle, bottom)
        let mid_price = (max_price + min_price) / 2.0;
        buf.set_string(
            inner.x,
            inner.y,
            &format!("{:>9.1}", max_price),
            Style::default().fg(Color::DarkGray),
        );
        if chart_height > 2 {
            buf.set_string(
                inner.x,
                inner.y + (chart_height / 2) as u16,
                &format!("{:>9.1}", mid_price),
                Style::default().fg(Color::DarkGray),
            );
        }
        buf.set_string(
            inner.x,
            inner.y + (chart_height - 1) as u16,
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
    }
}
