use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::model::order::OrderSide;
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;

pub struct PositionPanel<'a> {
    position: &'a Position,
    current_price: Option<f64>,
    balances: &'a HashMap<String, f64>,
}

impl<'a> PositionPanel<'a> {
    pub fn new(
        position: &'a Position,
        current_price: Option<f64>,
        balances: &'a HashMap<String, f64>,
    ) -> Self {
        Self {
            position,
            current_price,
            balances,
        }
    }
}

impl Widget for PositionPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let side_str = match self.position.side {
            Some(OrderSide::Buy) => "LONG",
            Some(OrderSide::Sell) => "SHORT",
            None => "FLAT",
        };
        let side_color = match self.position.side {
            Some(OrderSide::Buy) => Color::Green,
            Some(OrderSide::Sell) => Color::Red,
            None => Color::DarkGray,
        };

        let pnl_color = |val: f64| {
            if val > 0.0 {
                Color::Green
            } else if val < 0.0 {
                Color::Red
            } else {
                Color::White
            }
        };

        let price_str = self
            .current_price
            .map(|p| format!("{:.2}", p))
            .unwrap_or_else(|| "---".to_string());

        let usdt_bal = self.balances.get("USDT").copied().unwrap_or(0.0);
        let btc_bal = self.balances.get("BTC").copied().unwrap_or(0.0);

        let lines = vec![
            Line::from(vec![
                Span::styled("USDT: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.2}", usdt_bal),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("BTC:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.5}", btc_bal),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(Span::styled(
                "──────────────────────",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::styled("Price:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", price_str),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Side: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    side_str,
                    Style::default()
                        .fg(side_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Qty:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.5}", self.position.qty),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Entry:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {:.2}", self.position.entry_price),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("UnrPL:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {:.4}", self.position.unrealized_pnl),
                    Style::default().fg(pnl_color(self.position.unrealized_pnl)),
                ),
            ]),
            Line::from(vec![
                Span::styled("RlzPL:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {:.4}", self.position.realized_pnl),
                    Style::default().fg(pnl_color(self.position.realized_pnl)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Trades:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", self.position.trade_count),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];

        let block = Block::default()
            .title(" Position ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        Paragraph::new(lines).block(block).render(area, buf);
    }
}

pub struct OrderLogPanel<'a> {
    last_signal: &'a Option<Signal>,
    last_order: &'a Option<OrderUpdate>,
    fast_sma: Option<f64>,
    slow_sma: Option<f64>,
}

impl<'a> OrderLogPanel<'a> {
    pub fn new(
        last_signal: &'a Option<Signal>,
        last_order: &'a Option<OrderUpdate>,
        fast_sma: Option<f64>,
        slow_sma: Option<f64>,
    ) -> Self {
        Self {
            last_signal,
            last_order,
            fast_sma,
            slow_sma,
        }
    }
}

impl Widget for OrderLogPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let signal_str = match self.last_signal {
            Some(Signal::Buy) => "BUY".to_string(),
            Some(Signal::Sell) => "SELL".to_string(),
            Some(Signal::Hold) | None => "---".to_string(),
        };

        let order_str = match self.last_order {
            Some(OrderUpdate::Filled {
                client_order_id,
                avg_price,
                ..
            }) => format!(
                "FILLED {} @ {:.2}",
                &client_order_id[..client_order_id.len().min(12)],
                avg_price
            ),
            Some(OrderUpdate::Submitted {
                client_order_id, ..
            }) => format!(
                "SUBMITTED {}",
                &client_order_id[..client_order_id.len().min(12)]
            ),
            Some(OrderUpdate::Rejected { reason, .. }) => {
                format!("REJECTED: {}", &reason[..reason.len().min(30)])
            }
            None => "---".to_string(),
        };

        let fast_str = self
            .fast_sma
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "---".to_string());
        let slow_str = self
            .slow_sma
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "---".to_string());

        let lines = vec![
            Line::from(vec![
                Span::styled("Signal: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&signal_str, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Order:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&order_str, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Fast SMA: ", Style::default().fg(Color::Green)),
                Span::styled(&fast_str, Style::default().fg(Color::White)),
                Span::styled("  Slow SMA: ", Style::default().fg(Color::Yellow)),
                Span::styled(&slow_str, Style::default().fg(Color::White)),
            ]),
        ];

        let block = Block::default()
            .title(" Orders & Signals ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        Paragraph::new(lines).block(block).render(area, buf);
    }
}

pub struct StatusBar<'a> {
    pub symbol: &'a str,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub timeframe: &'a str,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let conn_status = if self.ws_connected {
            Span::styled("CONNECTED", Style::default().fg(Color::Green))
        } else {
            Span::styled(
                "DISCONNECTED",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )
        };

        let pause_status = if self.paused {
            Span::styled(
                " STRAT OFF ",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" STRAT ON ", Style::default().fg(Color::Green))
        };

        let line = Line::from(vec![
            Span::styled(
                " sandbox-quant ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.symbol, Style::default().fg(Color::Cyan)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.timeframe.to_uppercase(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            conn_status,
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            pause_status,
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("ticks: {}", self.tick_count),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Scrolling order history panel that shows recent order events.
pub struct OrderHistoryPanel<'a> {
    messages: &'a [String],
}

impl<'a> OrderHistoryPanel<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self { messages }
    }
}

impl Widget for OrderHistoryPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Order History ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner_height = block.inner(area).height as usize;

        let visible: Vec<Line> = self
            .messages
            .iter()
            .rev()
            .take(inner_height)
            .rev()
            .map(|msg| {
                let color = if msg.contains("REJECTED") {
                    Color::Red
                } else if msg.contains("FILLED") {
                    Color::Green
                } else if msg.contains("SUBMITTED") {
                    Color::Cyan
                } else {
                    Color::DarkGray
                };
                Line::from(Span::styled(msg.as_str(), Style::default().fg(color)))
            })
            .collect();

        Paragraph::new(visible)
            .block(block)
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}

/// Scrolling system log panel that shows recent events.
pub struct LogPanel<'a> {
    messages: &'a [String],
}

impl<'a> LogPanel<'a> {
    pub fn new(messages: &'a [String]) -> Self {
        Self { messages }
    }
}

impl Widget for LogPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" System Log ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner_height = block.inner(area).height as usize;

        // Take the last N messages that fit in the panel
        let visible: Vec<Line> = self
            .messages
            .iter()
            .rev()
            .take(inner_height)
            .rev()
            .map(|msg| {
                let (color, text) = if msg.starts_with("[ERR]") {
                    (Color::Red, msg.as_str())
                } else if msg.starts_with("[WARN]") {
                    (Color::Yellow, msg.as_str())
                } else if msg.contains("FILLED") || msg.contains("Connected") {
                    (Color::Green, msg.as_str())
                } else {
                    (Color::DarkGray, msg.as_str())
                };
                Line::from(Span::styled(text, Style::default().fg(color)))
            })
            .collect();

        Paragraph::new(visible)
            .block(block)
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}

pub struct KeybindBar;

impl Widget for KeybindBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let line = Line::from(vec![
            Span::styled(" [Q]", Style::default().fg(Color::Yellow)),
            Span::styled("uit ", Style::default().fg(Color::DarkGray)),
            Span::styled("[P]", Style::default().fg(Color::Yellow)),
            Span::styled("ause ", Style::default().fg(Color::DarkGray)),
            Span::styled("[R]", Style::default().fg(Color::Yellow)),
            Span::styled("esume ", Style::default().fg(Color::DarkGray)),
            Span::styled("[B]", Style::default().fg(Color::Green)),
            Span::styled("uy ", Style::default().fg(Color::DarkGray)),
            Span::styled("[S]", Style::default().fg(Color::Red)),
            Span::styled("ell ", Style::default().fg(Color::DarkGray)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("[1]", Style::default().fg(Color::Cyan)),
            Span::styled("min ", Style::default().fg(Color::DarkGray)),
            Span::styled("[H]", Style::default().fg(Color::Cyan)),
            Span::styled("our ", Style::default().fg(Color::DarkGray)),
            Span::styled("[D]", Style::default().fg(Color::Cyan)),
            Span::styled("ay ", Style::default().fg(Color::DarkGray)),
            Span::styled("[W]", Style::default().fg(Color::Cyan)),
            Span::styled("eek ", Style::default().fg(Color::DarkGray)),
            Span::styled("[M]", Style::default().fg(Color::Cyan)),
            Span::styled("onth ", Style::default().fg(Color::DarkGray)),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}
