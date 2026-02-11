use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};

pub struct PriceChart<'a> {
    prices: &'a [f64],
    fast_sma: Option<f64>,
    slow_sma: Option<f64>,
    symbol: &'a str,
}

impl<'a> PriceChart<'a> {
    pub fn new(prices: &'a [f64], symbol: &'a str) -> Self {
        Self {
            prices,
            fast_sma: None,
            slow_sma: None,
            symbol,
        }
    }

    pub fn fast_sma(mut self, val: Option<f64>) -> Self {
        self.fast_sma = val;
        self
    }

    pub fn slow_sma(mut self, val: Option<f64>) -> Self {
        self.slow_sma = val;
        self
    }
}

impl Widget for PriceChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build title with current price
        let title = if let Some(&last) = self.prices.last() {
            format!(" {} | {:.2} ", self.symbol, last)
        } else {
            format!(" {} | --- ", self.symbol)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        if self.prices.is_empty() || inner.height < 3 || inner.width < 10 {
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

        // Reserve 10 chars on the left for price labels
        let label_width: u16 = 10;
        let chart_x_start = inner.x + label_width;
        let chart_width = inner.width.saturating_sub(label_width) as usize;
        let chart_height = inner.height as usize;

        if chart_width < 2 || chart_height < 2 {
            return;
        }

        // Take the last `chart_width` prices
        let visible: Vec<f64> = if self.prices.len() > chart_width {
            self.prices[self.prices.len() - chart_width..].to_vec()
        } else {
            self.prices.to_vec()
        };

        let min_price = visible.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_price = visible.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = max_price - min_price;
        let range = if range < 0.01 { 1.0 } else { range };

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

        // Draw price dots
        for (i, &price) in visible.iter().enumerate() {
            let x = chart_x_start + i as u16;
            if x >= inner.x + inner.width {
                break;
            }
            let normalized = (price - min_price) / range;
            let y_pos = ((1.0 - normalized) * (chart_height - 1) as f64).round() as u16;
            let y = inner.y + y_pos.min(inner.height - 1);

            buf.set_string(x, y, "·", Style::default().fg(Color::Cyan));
        }

        // Highlight the most recent price dot
        if let Some(&last_price) = visible.last() {
            let x = chart_x_start + (visible.len() - 1).min(chart_width - 1) as u16;
            let normalized = (last_price - min_price) / range;
            let y_pos = ((1.0 - normalized) * (chart_height - 1) as f64).round() as u16;
            let y = inner.y + y_pos.min(inner.height - 1);
            if x < inner.x + inner.width {
                buf.set_string(
                    x,
                    y,
                    "●",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }

        // Draw SMA markers on the rightmost column
        let sma_x = chart_x_start + visible.len().min(chart_width).saturating_sub(1) as u16;
        if sma_x < inner.x + inner.width {
            if let Some(fast) = self.fast_sma {
                if fast >= min_price && fast <= max_price {
                    let normalized = (fast - min_price) / range;
                    let y_pos = ((1.0 - normalized) * (chart_height - 1) as f64).round() as u16;
                    let y = inner.y + y_pos.min(inner.height - 1);
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
                    let normalized = (slow - min_price) / range;
                    let y_pos = ((1.0 - normalized) * (chart_height - 1) as f64).round() as u16;
                    let y = inner.y + y_pos.min(inner.height - 1);
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
