use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

pub struct PriceChart<'a> {
    prices: &'a [f64],
    fast_sma: Option<f64>,
    slow_sma: Option<f64>,
}

impl<'a> PriceChart<'a> {
    pub fn new(prices: &'a [f64]) -> Self {
        Self {
            prices,
            fast_sma: None,
            slow_sma: None,
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
        let block = Block::default()
            .title(" Price (BTCUSDT) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        if self.prices.is_empty() || inner.height < 2 || inner.width < 4 {
            return;
        }

        let chart_height = inner.height.saturating_sub(1) as usize; // leave 1 row for axis labels
        let chart_width = inner.width as usize;

        // Take the last `chart_width` prices
        let visible: Vec<f64> = if self.prices.len() > chart_width {
            self.prices[self.prices.len() - chart_width..].to_vec()
        } else {
            self.prices.to_vec()
        };

        if visible.is_empty() {
            return;
        }

        let min_price = visible.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_price = visible.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let range = max_price - min_price;
        let range = if range < 0.01 { 1.0 } else { range };

        // Draw price line
        for (i, &price) in visible.iter().enumerate() {
            let x = inner.x + i as u16;
            if x >= inner.x + inner.width {
                break;
            }
            let normalized = (price - min_price) / range;
            let y_pos = chart_height - 1 - ((normalized * (chart_height - 1) as f64) as usize).min(chart_height - 1);
            let y = inner.y + y_pos as u16;

            if y < inner.y + inner.height {
                buf.set_string(x, y, "●", Style::default().fg(Color::Cyan));
            }
        }

        // Draw SMA indicators on last column
        let last_x = inner.x + visible.len().min(chart_width) as u16 - 1;
        if let Some(fast) = self.fast_sma {
            if fast >= min_price && fast <= max_price {
                let normalized = (fast - min_price) / range;
                let y_pos = chart_height - 1 - ((normalized * (chart_height - 1) as f64) as usize).min(chart_height - 1);
                let y = inner.y + y_pos as u16;
                if y < inner.y + inner.height && last_x < inner.x + inner.width {
                    buf.set_string(last_x, y, "F", Style::default().fg(Color::Green));
                }
            }
        }
        if let Some(slow) = self.slow_sma {
            if slow >= min_price && slow <= max_price {
                let normalized = (slow - min_price) / range;
                let y_pos = chart_height - 1 - ((normalized * (chart_height - 1) as f64) as usize).min(chart_height - 1);
                let y = inner.y + y_pos as u16;
                if y < inner.y + inner.height && last_x < inner.x + inner.width {
                    buf.set_string(last_x, y, "S", Style::default().fg(Color::Yellow));
                }
            }
        }

        // Axis labels
        let label_y = inner.y + inner.height - 1;
        let max_label = format!("{:.1}", max_price);
        let min_label = format!("{:.1}", min_price);
        buf.set_string(
            inner.x,
            inner.y,
            &max_label,
            Style::default().fg(Color::DarkGray),
        );
        buf.set_string(
            inner.x,
            label_y,
            &min_label,
            Style::default().fg(Color::DarkGray),
        );
    }
}
