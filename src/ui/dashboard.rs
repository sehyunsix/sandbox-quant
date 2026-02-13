use std::collections::HashMap;

use chrono::{Local, TimeZone};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::alpaca::rest::OptionChainSnapshot;
use crate::model::order::OrderSide;
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;
use crate::strategy_stats::StrategyStats;

pub struct PositionPanel<'a> {
    position: &'a Position,
    current_price: Option<f64>,
    balances: &'a HashMap<String, f64>,
    strategy_label: &'a str,
    strategy_stats: Option<StrategyStats>,
}

impl<'a> PositionPanel<'a> {
    pub fn new(
        position: &'a Position,
        current_price: Option<f64>,
        balances: &'a HashMap<String, f64>,
        strategy_label: &'a str,
        strategy_stats: Option<StrategyStats>,
    ) -> Self {
        Self {
            position,
            current_price,
            balances,
            strategy_label,
            strategy_stats,
        }
    }

    fn win_rate_text(position: &Position) -> String {
        format!("{:.1}%", position.win_rate_percent())
    }

    fn win_loss_text(position: &Position) -> String {
        format!(
            "{}/{}",
            position.winning_trade_count, position.losing_trade_count
        )
    }

    fn strategy_win_rate_text(strategy_stats: Option<StrategyStats>) -> String {
        match strategy_stats {
            Some(stats) => format!("{:.1}%", stats.win_rate_percent()),
            None => "0.0%".to_string(),
        }
    }

    fn strategy_win_loss_text(strategy_stats: Option<StrategyStats>) -> String {
        match strategy_stats {
            Some(stats) => format!("{}/{}", stats.wins, stats.losses),
            None => "0/0".to_string(),
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
                    Style::default().fg(side_color).add_modifier(Modifier::BOLD),
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
            Line::from(vec![
                Span::styled("WinRate:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", Self::win_rate_text(self.position)),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("W/L:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    Self::win_loss_text(self.position),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Strat: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.strategy_label, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("S WinRate:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", Self::strategy_win_rate_text(self.strategy_stats)),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("S W/L: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    Self::strategy_win_loss_text(self.strategy_stats),
                    Style::default().fg(Color::Cyan),
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

#[cfg(test)]
mod tests {
    use super::PositionPanel;
    use crate::model::position::Position;
    use crate::strategy_stats::StrategyStats;

    #[test]
    fn win_loss_text_formats_counts() {
        let mut position = Position::new("BTCUSDT".to_string());
        position.winning_trade_count = 12;
        position.losing_trade_count = 3;

        assert_eq!(PositionPanel::win_loss_text(&position), "12/3");
    }

    #[test]
    fn win_rate_text_formats_percent() {
        let mut position = Position::new("BTCUSDT".to_string());
        position.winning_trade_count = 3;
        position.losing_trade_count = 1;

        assert_eq!(PositionPanel::win_rate_text(&position), "75.0%");
    }

    #[test]
    fn strategy_win_rate_text_formats_percent() {
        assert_eq!(
            PositionPanel::strategy_win_rate_text(Some(StrategyStats { wins: 2, losses: 1 })),
            "66.7%"
        );
    }

    #[test]
    fn strategy_win_loss_defaults_when_missing() {
        assert_eq!(PositionPanel::strategy_win_loss_text(None), "0/0");
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
            Some(Signal::Buy { .. }) => "BUY".to_string(),
            Some(Signal::Sell { .. }) => "SELL".to_string(),
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
    pub product_label: &'a str,
    pub strategy_label: &'a str,
    pub ws_connected: bool,
    pub paused: bool,
    pub tick_count: u64,
    pub last_market_update_ms: Option<u64>,
    pub timeframe: &'a str,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let now_str = Local::now().format("%H:%M:%S").to_string();
        let last_update_str = self
            .last_market_update_ms
            .and_then(|ts| Local.timestamp_millis_opt(ts as i64).single())
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "---".to_string());

        let conn_status = if self.ws_connected {
            Span::styled("CONNECTED", Style::default().fg(Color::Green))
        } else {
            Span::styled(
                "DISCONNECTED",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        };

        let pause_status = if self.paused {
            Span::styled(
                " STRAT OFF ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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
                self.product_label,
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.strategy_label, Style::default().fg(Color::Cyan)),
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
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("now: {}", now_str),
                Style::default().fg(Color::White),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("last: {}", last_update_str),
                Style::default().fg(Color::Yellow),
            ),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Scrolling order history panel that shows recent order events.
pub struct OrderHistoryPanel<'a> {
    messages: &'a [String],
    scroll_offset: usize,
}

impl<'a> OrderHistoryPanel<'a> {
    pub fn new(messages: &'a [String], scroll_offset: usize) -> Self {
        Self {
            messages,
            scroll_offset,
        }
    }
}

impl Widget for OrderHistoryPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Order History ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner_height = block.inner(area).height as usize;

        let len = self.messages.len();
        let end = len.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(inner_height);

        let visible: Vec<Line> = self.messages[start..end]
            .iter()
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
            Span::styled("[0]", Style::default().fg(Color::Cyan)),
            Span::styled("sec ", Style::default().fg(Color::DarkGray)),
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
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("[T]", Style::default().fg(Color::Magenta)),
            Span::styled(" product ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Y]", Style::default().fg(Color::Magenta)),
            Span::styled(" strategy ", Style::default().fg(Color::DarkGray)),
            Span::styled("[A]", Style::default().fg(Color::Magenta)),
            Span::styled("ccount ", Style::default().fg(Color::DarkGray)),
            Span::styled("[N]", Style::default().fg(Color::Magenta)),
            Span::styled("extSym ", Style::default().fg(Color::DarkGray)),
            Span::styled("[V]", Style::default().fg(Color::Magenta)),
            Span::styled("revSym ", Style::default().fg(Color::DarkGray)),
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("[J]", Style::default().fg(Color::Cyan)),
            Span::styled("/[K]", Style::default().fg(Color::Cyan)),
            Span::styled(" history ", Style::default().fg(Color::DarkGray)),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}

pub struct AccountPanel<'a> {
    pub symbol: &'a str,
    pub product_label: &'a str,
    pub position: &'a Position,
    pub balances: &'a HashMap<String, f64>,
    pub strategy_stats: &'a HashMap<String, StrategyStats>,
}

impl AccountPanel<'_> {
    fn split_symbol_assets(symbol: &str) -> (&str, &str) {
        for quote in ["USDT", "USDC", "BUSD", "FDUSD", "BTC", "ETH", "BNB"] {
            if let Some(base) = symbol.strip_suffix(quote) {
                if !base.is_empty() {
                    return (base, quote);
                }
            }
        }
        (symbol, "USDT")
    }
}

impl Widget for AccountPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Account (A/Esc close) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));
        let inner = block.inner(area);
        block.render(area, buf);

        let side = match self.position.side {
            Some(OrderSide::Buy) => "LONG",
            Some(OrderSide::Sell) => "SHORT",
            None => "FLAT",
        };
        let (base_asset, quote_asset) = Self::split_symbol_assets(self.symbol);
        let base_balance = self.balances.get(base_asset).copied().unwrap_or(0.0);
        let quote_balance = self.balances.get(quote_asset).copied().unwrap_or(0.0);
        let bought_str = if base_balance > 0.0 {
            format!("{:.5} {}", base_balance, base_asset)
        } else {
            "None".to_string()
        };

        let mut holdings: Vec<(&str, f64)> = self
            .balances
            .iter()
            .filter_map(|(asset, qty)| (*qty > 0.0).then_some((asset.as_str(), *qty)))
            .collect();
        holdings.sort_by(|a, b| a.0.cmp(b.0));

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Product: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.product_label, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Symbol:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.symbol, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(Span::styled(
                "────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::styled("Position:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", side),
                    Style::default()
                        .fg(if side == "LONG" {
                            Color::Green
                        } else if side == "SHORT" {
                            Color::Red
                        } else {
                            Color::DarkGray
                        })
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Qty:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.5} {}", self.position.qty, base_asset),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Bought:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(bought_str, Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Balances:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        " {:.2} {} / {:.5} {}",
                        quote_balance, quote_asset, base_balance, base_asset
                    ),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];

        if !holdings.is_empty() {
            lines.push(Line::from(Span::styled(
                "Assets:",
                Style::default().fg(Color::DarkGray),
            )));
            for (asset, qty) in holdings
                .into_iter()
                .take(inner.height.saturating_sub(9) as usize)
            {
                lines.push(Line::from(vec![
                    Span::styled("  - ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{:<6} {:.8}", asset, qty),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }
        }

        if !self.strategy_stats.is_empty() {
            let mut stats_rows: Vec<(&str, StrategyStats)> = self
                .strategy_stats
                .iter()
                .map(|(k, v)| (k.as_str(), *v))
                .collect();
            stats_rows.sort_by(|a, b| a.0.cmp(b.0));

            lines.push(Line::from(Span::styled(
                "Strategy Win Rates:",
                Style::default().fg(Color::DarkGray),
            )));
            for (name, stats) in stats_rows
                .into_iter()
                .take(inner.height.saturating_sub(lines.len() as u16 + 1) as usize)
            {
                lines.push(Line::from(vec![
                    Span::styled("  - ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(
                            "{:<14} {:>5.1}% ({}/{})",
                            name,
                            stats.win_rate_percent(),
                            stats.wins,
                            stats.losses
                        ),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }
        }

        Paragraph::new(lines)
            .block(Block::default())
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}

pub struct OptionPanel<'a> {
    pub chain: Option<&'a OptionChainSnapshot>,
}

impl Widget for OptionPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Option ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        let fmt = |v: Option<f64>, width: usize, digits: usize| match v {
            Some(x) => format!("{x:>width$.digits$}", width = width, digits = digits),
            None => format!("{:>width$}", "---", width = width),
        };

        let mut lines = vec![Line::from(vec![
            Span::styled("CP", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Strike", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Theo", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Bid", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Ask", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Δ", Style::default().fg(Color::DarkGray)),
            Span::styled(" ", Style::default()),
            Span::styled("Θ", Style::default().fg(Color::DarkGray)),
        ])];

        if let Some(chain) = self.chain {
            let max_rows = inner.height.saturating_sub(2) as usize;
            for row in chain.rows.iter().take(max_rows) {
                let cp = if row.option_type == "CALL" { "C" } else { "P" };
                let strike = match row.strike {
                    Some(v) => format!("{v:>7.2}"),
                    None => "   --- ".to_string(),
                };
                let rest = format!(
                    "{} {} {} {} {} {}",
                    strike,
                    fmt(row.theoretical_price, 6, 2),
                    fmt(row.bid, 6, 2),
                    fmt(row.ask, 6, 2),
                    fmt(row.delta, 5, 2),
                    fmt(row.theta, 5, 2)
                );
                let cp_color = if cp == "C" { Color::Green } else { Color::Red };
                lines.push(Line::from(vec![
                    Span::styled(
                        cp.to_string(),
                        Style::default().fg(cp_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {}", rest), Style::default().fg(Color::White)),
                ]));
            }
            lines.push(Line::from(Span::styled(
                format!("Underly: {}", chain.underlying),
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "No option chain snapshot",
                Style::default().fg(Color::DarkGray),
            )));
        }

        Paragraph::new(lines)
            .block(Block::default())
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }
}

pub struct ProductSelectorPanel<'a> {
    pub items: &'a [&'a str],
    pub selected: usize,
}

impl Widget for ProductSelectorPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Select Product (Up/Down + Enter, Esc cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));
        let inner = block.inner(area);
        block.render(area, buf);

        for (idx, item) in self.items.iter().enumerate() {
            let y = inner.y + idx as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_selected = idx == self.selected;
            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(inner.x + 1, y, format!("{}{}", prefix, item), style);
        }
    }
}

pub struct StrategySelectorPanel<'a> {
    pub items: &'a [&'a str],
    pub selected: usize,
}

impl Widget for StrategySelectorPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Select Strategy (Up/Down + Enter, Esc cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));
        let inner = block.inner(area);
        block.render(area, buf);

        for (idx, item) in self.items.iter().enumerate() {
            let y = inner.y + idx as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let is_selected = idx == self.selected;
            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            buf.set_string(inner.x + 1, y, format!("{}{}", prefix, item), style);
        }
    }
}
