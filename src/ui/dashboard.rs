use chrono::TimeZone;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::event::{EvSnapshotEntry, ExitPolicyEntry};
use crate::model::order::OrderSide;
use crate::model::position::Position;
use crate::model::signal::Signal;
use crate::order_manager::OrderUpdate;

pub struct PositionPanel<'a> {
    position: &'a Position,
    current_price: Option<f64>,
    last_applied_fee: &'a str,
    ev_snapshot: Option<&'a EvSnapshotEntry>,
    exit_policy: Option<&'a ExitPolicyEntry>,
}

impl<'a> PositionPanel<'a> {
    pub fn new(
        position: &'a Position,
        current_price: Option<f64>,
        last_applied_fee: &'a str,
        ev_snapshot: Option<&'a EvSnapshotEntry>,
        exit_policy: Option<&'a ExitPolicyEntry>,
    ) -> Self {
        Self {
            position,
            current_price,
            last_applied_fee,
            ev_snapshot,
            exit_policy,
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

        let lines = vec![
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
                Span::styled("Fee:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.last_applied_fee, Style::default().fg(Color::LightBlue)),
            ]),
            Line::from(vec![
                Span::styled("EV@entry: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.ev_snapshot
                        .map(|e| format!("{:+.4}", e.ev))
                        .unwrap_or_else(|| "--".to_string()),
                    Style::default().fg(self.ev_snapshot.map_or(Color::DarkGray, |e| {
                        if e.ev > 0.0 {
                            Color::Green
                        } else if e.ev < 0.0 {
                            Color::Red
                        } else {
                            Color::White
                        }
                    })),
                ),
                Span::styled("  pW@entry:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.ev_snapshot
                        .map(|e| format!("{:.2}", e.p_win))
                        .unwrap_or_else(|| "--".to_string()),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Gate: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.ev_snapshot
                        .map(|e| {
                            if e.gate_blocked {
                                format!("{} (BLOCK)", e.gate_mode)
                            } else {
                                e.gate_mode.clone()
                            }
                        })
                        .unwrap_or_else(|| "--".to_string()),
                    Style::default().fg(self.ev_snapshot.map_or(Color::DarkGray, |e| {
                        if e.gate_blocked {
                            Color::Red
                        } else if e.gate_mode.eq_ignore_ascii_case("soft") {
                            Color::Yellow
                        } else {
                            Color::White
                        }
                    })),
                ),
            ]),
            Line::from(vec![
                Span::styled("Stop: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.exit_policy
                        .and_then(|p| p.stop_price)
                        .map(|v| format!("{:.2}", v))
                        .unwrap_or_else(|| "--".to_string()),
                    Style::default().fg(self.exit_policy.map_or(Color::DarkGray, |p| {
                        match p.protective_stop_ok {
                            Some(true) => Color::Green,
                            Some(false) => Color::Red,
                            None => Color::Yellow,
                        }
                    })),
                ),
                Span::styled("  Hold:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.exit_policy
                        .and_then(|p| p.expected_holding_ms)
                        .map(|v| format!("{}s", v / 1000))
                        .unwrap_or_else(|| "--".to_string()),
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

pub struct StrategyMetricsPanel<'a> {
    strategy_label: &'a str,
    trade_count: u32,
    win_count: u32,
    lose_count: u32,
    realized_pnl: f64,
}

impl<'a> StrategyMetricsPanel<'a> {
    pub fn new(
        strategy_label: &'a str,
        trade_count: u32,
        win_count: u32,
        lose_count: u32,
        realized_pnl: f64,
    ) -> Self {
        Self {
            strategy_label,
            trade_count,
            win_count,
            lose_count,
            realized_pnl,
        }
    }
}

impl Widget for StrategyMetricsPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let pnl_color = if self.realized_pnl > 0.0 {
            Color::Green
        } else if self.realized_pnl < 0.0 {
            Color::Red
        } else {
            Color::White
        };
        let win_rate = if self.trade_count == 0 {
            0.0
        } else {
            (self.win_count as f64 / self.trade_count as f64) * 100.0
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("Strategy: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    self.strategy_label,
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Trades: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.trade_count.to_string(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Win: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.win_count.to_string(), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Lose: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.lose_count.to_string(), Style::default().fg(Color::Red)),
            ]),
            Line::from(vec![
                Span::styled("WinRate: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:.1}%", win_rate), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("RlzPL: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:+.4}", self.realized_pnl),
                    Style::default().fg(pnl_color),
                ),
            ]),
        ];

        let block = Block::default()
            .title(" Strategy Metrics ")
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
    trade_count: u32,
    win_count: u32,
    lose_count: u32,
    realized_pnl: f64,
}

impl<'a> OrderLogPanel<'a> {
    pub fn new(
        last_signal: &'a Option<Signal>,
        last_order: &'a Option<OrderUpdate>,
        fast_sma: Option<f64>,
        slow_sma: Option<f64>,
        trade_count: u32,
        win_count: u32,
        lose_count: u32,
        realized_pnl: f64,
    ) -> Self {
        Self {
            last_signal,
            last_order,
            fast_sma,
            slow_sma,
            trade_count,
            win_count,
            lose_count,
            realized_pnl,
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
            Line::from(vec![
                Span::styled("Trades: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", self.trade_count),
                    Style::default().fg(Color::White),
                ),
                Span::styled("  Win: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", self.win_count),
                    Style::default().fg(Color::Green),
                ),
                Span::styled("  Lose: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", self.lose_count),
                    Style::default().fg(Color::Red),
                ),
                Span::styled("  PnL: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.4}", self.realized_pnl),
                    Style::default().fg(if self.realized_pnl >= 0.0 {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
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
    pub strategy_label: &'a str,
    pub ws_connected: bool,
    pub paused: bool,
    pub timeframe: &'a str,
    pub last_price_update_ms: Option<u64>,
    pub last_price_latency_ms: Option<u64>,
    pub last_order_history_update_ms: Option<u64>,
    pub last_order_history_latency_ms: Option<u64>,
    pub close_all_status: Option<&'a str>,
    pub close_all_running: bool,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let fmt_update = |ts_ms: Option<u64>| -> String {
            ts_ms
                .and_then(|ts| chrono::Utc.timestamp_millis_opt(ts as i64).single())
                .map(|dt| {
                    dt.with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string()
                })
                .unwrap_or_else(|| "--:--:--".to_string())
        };
        let fmt_age = |lat_ms: Option<u64>| -> String {
            lat_ms
                .map(|v| format!("{}ms", v))
                .unwrap_or_else(|| "--".to_string())
        };

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

        let mut spans = vec![
            Span::styled(
                " sandbox-quant ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.symbol, Style::default().fg(Color::Cyan)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(self.strategy_label, Style::default().fg(Color::Magenta)),
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
                format!(
                    "updated:{} lat:{}",
                    fmt_update(self.last_price_update_ms),
                    fmt_age(self.last_price_latency_ms)
                ),
                Style::default().fg(Color::Blue),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "order-updated:{} lat:{}",
                    fmt_update(self.last_order_history_update_ms),
                    fmt_age(self.last_order_history_latency_ms)
                ),
                Style::default().fg(Color::Cyan),
            ),
        ];
        if let Some(status) = self.close_all_status {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                status,
                Style::default().fg(if self.close_all_running {
                    Color::Yellow
                } else {
                    Color::LightGreen
                }),
            ));
        }
        let line = Line::from(spans);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Scrolling order history panel that shows recent order events.
pub struct OrderHistoryPanel<'a> {
    open_messages: &'a [String],
    filled_messages: &'a [String],
}

impl<'a> OrderHistoryPanel<'a> {
    pub fn new(open_messages: &'a [String], filled_messages: &'a [String]) -> Self {
        Self {
            open_messages,
            filled_messages,
        }
    }
}

impl Widget for OrderHistoryPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Order History ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        block.render(area, buf);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let render_list = |title: &str, messages: &[String], area: Rect, buf: &mut Buffer| {
            let sub_block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray));
            let inner_height = sub_block.inner(area).height as usize;
            let visible: Vec<Line> = messages
                .iter()
                .rev()
                .take(inner_height)
                .rev()
                .map(|msg| {
                    let color = if msg.contains("REJECTED") {
                        Color::Red
                    } else if msg.contains("FILLED") {
                        Color::Green
                    } else if msg.contains("SUBMITTED") || msg.contains("PARTIALLY_FILLED") {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    };
                    Line::from(Span::styled(msg.as_str(), Style::default().fg(color)))
                })
                .collect();

            Paragraph::new(visible)
                .block(sub_block)
                .wrap(Wrap { trim: true })
                .render(area, buf);
        };

        render_list(" Open ", self.open_messages, cols[0], buf);
        render_list(" Filled ", self.filled_messages, cols[1], buf);
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
            Span::styled("quit ", Style::default().fg(Color::DarkGray)),
            Span::styled("[P]", Style::default().fg(Color::Yellow)),
            Span::styled("/[R] ", Style::default().fg(Color::DarkGray)),
            Span::styled("pause/resume ", Style::default().fg(Color::DarkGray)),
            Span::styled("[B]", Style::default().fg(Color::Green)),
            Span::styled("/[S] ", Style::default().fg(Color::DarkGray)),
            Span::styled("buy/sell ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Z] ", Style::default().fg(Color::Red)),
            Span::styled("close-all ", Style::default().fg(Color::DarkGray)),
            Span::styled("[G]", Style::default().fg(Color::Magenta)),
            Span::styled(" grid ", Style::default().fg(Color::DarkGray)),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled("TF:", Style::default().fg(Color::Cyan)),
            Span::styled(" 0/1/H/D/W/M ", Style::default().fg(Color::DarkGray)),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled("More:", Style::default().fg(Color::Magenta)),
            Span::styled(" T/Y/A/I ", Style::default().fg(Color::DarkGray)),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}

pub struct GridKeybindBar;

impl Widget for GridKeybindBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let line = Line::from(vec![
            Span::styled(" [Q]", Style::default().fg(Color::Yellow)),
            Span::styled("quit ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
            Span::styled(" panel ", Style::default().fg(Color::DarkGray)),
            Span::styled("[J/K]", Style::default().fg(Color::Yellow)),
            Span::styled(" select ", Style::default().fg(Color::DarkGray)),
            Span::styled("[H/L]", Style::default().fg(Color::Yellow)),
            Span::styled(" symbol ", Style::default().fg(Color::DarkGray)),
            Span::styled("[O]", Style::default().fg(Color::Yellow)),
            Span::styled(" toggle ", Style::default().fg(Color::DarkGray)),
            Span::styled("[N]", Style::default().fg(Color::Yellow)),
            Span::styled(" new ", Style::default().fg(Color::DarkGray)),
            Span::styled("[C]", Style::default().fg(Color::Yellow)),
            Span::styled(" cfg ", Style::default().fg(Color::DarkGray)),
            Span::styled("[X]", Style::default().fg(Color::Yellow)),
            Span::styled(" del ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
            Span::styled(" run ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled(" close ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Z]", Style::default().fg(Color::Red)),
            Span::styled(" close-all ", Style::default().fg(Color::DarkGray)),
        ]);

        buf.set_line(area.x, area.y, &line, area.width);
    }
}
