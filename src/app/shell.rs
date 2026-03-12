use crate::app::bootstrap::{AppBootstrap, BinanceMode};
use crate::app::cli::{
    complete_shell_input_with_market_data, parse_shell_input, shell_help_text, ShellInput,
};
use crate::app::output::render_command_output;
use crate::app::runtime::AppRuntime;
use crate::exchange::binance::client::BinanceExchange;
use crate::terminal::app::{TerminalApp, TerminalEvent};
use crate::terminal::completion::ShellCompletion;
pub use crate::terminal::completion::{
    format_completion_line, next_completion_index, previous_completion_index, scroll_lines_needed,
};
use crate::terminal::loop_shell::run_terminal;
use crate::ui::operator_terminal::{
    mode_name, operator_prompt, prompt_status_from_store, shell_intro_panel,
};
use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub fn run_shell(
    app: &mut AppBootstrap<BinanceExchange>,
    runtime: &mut AppRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = OperatorTerminal { app, runtime };
    run_terminal(&mut terminal)
}

struct OperatorTerminal<'a> {
    app: &'a mut AppBootstrap<BinanceExchange>,
    runtime: &'a mut AppRuntime,
}

impl TerminalApp for OperatorTerminal<'_> {
    fn intro_panel(&self) -> String {
        shell_intro_panel(mode_name(current_mode(self.app)), "~/project/sandbox-quant")
    }

    fn help_text(&self) -> String {
        shell_help_text().to_string()
    }

    fn prompt(&self) -> String {
        let mode = current_mode(self.app);
        let status = prompt_status(self.app);
        operator_prompt(mode, &status)
    }

    fn complete(&self, line: &str) -> Vec<ShellCompletion> {
        current_completions(self.app, line)
    }

    fn execute_line(&mut self, line: &str) -> Result<TerminalEvent, String> {
        match parse_shell_input(line) {
            Ok(ShellInput::Empty) => Ok(TerminalEvent::NoOutput),
            Ok(ShellInput::Help) => Ok(TerminalEvent::Output(shell_help_text().to_string())),
            Ok(ShellInput::Exit) => Ok(TerminalEvent::Exit),
            Ok(ShellInput::Mode(mode)) => self
                .app
                .switch_mode(mode)
                .map(|_| TerminalEvent::Output(format!("mode switched to {}", mode_name(mode))))
                .map_err(|error| error.to_string()),
            Ok(ShellInput::Command(command)) => {
                let rendered_command = command.clone();
                self.runtime
                    .run(self.app, command)
                    .map_err(|error| error.to_string())?;
                Ok(TerminalEvent::Output(render_command_output(
                    &rendered_command,
                    &self.app.portfolio_store,
                    &self.app.price_store,
                    &self.app.event_log,
                    &self.app.strategy_store,
                    self.app.mode,
                )))
            }
            Err(error) => Err(error),
        }
    }
}

fn current_mode(app: &AppBootstrap<BinanceExchange>) -> BinanceMode {
    app.mode
}

fn prompt_status(app: &AppBootstrap<BinanceExchange>) -> String {
    prompt_status_from_store(&app.portfolio_store)
}

fn current_completions(app: &AppBootstrap<BinanceExchange>, buffer: &str) -> Vec<ShellCompletion> {
    let mut instruments = completion_instruments(&app.portfolio_store, &app.event_log);
    if should_include_option_symbols(buffer) {
        instruments.extend(option_completion_symbols(&app.exchange));
        instruments.sort();
        instruments.dedup();
    }
    let priced_instruments = app
        .price_store
        .snapshot()
        .into_iter()
        .map(|(instrument, price)| (instrument.0, price))
        .collect::<Vec<_>>();
    complete_shell_input_with_market_data(buffer, &instruments, &priced_instruments)
}

#[derive(Debug, Clone)]
struct OptionCompletionCache {
    transport_name: String,
    fetched_at: Instant,
    symbols: Vec<String>,
}

fn option_completion_symbols(exchange: &BinanceExchange) -> Vec<String> {
    static CACHE: OnceLock<Mutex<Option<OptionCompletionCache>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let transport_name = exchange.transport_name().to_string();

    if let Some(cached) = cache
        .lock()
        .expect("lock option completion cache")
        .as_ref()
        .filter(|cached| {
            cached.transport_name == transport_name
                && cached.fetched_at.elapsed() < Duration::from_secs(300)
        })
        .cloned()
    {
        return cached.symbols;
    }

    let symbols = exchange.load_option_symbols().unwrap_or_default();
    *cache.lock().expect("lock option completion cache") = Some(OptionCompletionCache {
        transport_name,
        fetched_at: Instant::now(),
        symbols: symbols.clone(),
    });
    symbols
}

fn should_include_option_symbols(buffer: &str) -> bool {
    let trimmed = buffer.trim_start();
    let without_prefix = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let trailing_space = without_prefix.ends_with(' ');
    let parts: Vec<&str> = without_prefix.split_whitespace().collect();
    if parts.first().copied() != Some("option-order") {
        return false;
    }
    let arg_index = if trailing_space {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };
    arg_index <= 1
}

fn completion_instruments(
    store: &crate::portfolio::store::PortfolioStateStore,
    event_log: &crate::storage::event_log::EventLog,
) -> Vec<String> {
    let mut instruments = BTreeSet::new();

    for instrument in store.snapshot.positions.keys() {
        instruments.insert(instrument.0.clone());
    }

    for instrument in store.snapshot.open_orders.keys() {
        instruments.insert(instrument.0.clone());
    }

    for event in event_log.records.iter().rev() {
        if event.kind != "app.execution.completed" {
            continue;
        }
        if let Some(instrument) = event.payload["instrument"].as_str() {
            instruments.insert(instrument.to_string());
        }
    }

    instruments.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{completion_instruments, prompt_status_from_store};
    use crate::domain::balance::BalanceSnapshot;
    use crate::domain::instrument::Instrument;
    use crate::domain::market::Market;
    use crate::domain::order::{OpenOrder, OrderStatus};
    use crate::domain::position::{PositionSnapshot, Side};
    use crate::portfolio::store::PortfolioStateStore;
    use crate::storage::event_log::{log, EventLog};
    use serde_json::json;

    #[test]
    fn completion_instruments_include_positions_open_orders_and_recent_execution_symbols() {
        let mut store = PortfolioStateStore::default();
        store.apply_snapshot(crate::exchange::types::AuthoritativeSnapshot {
            balances: vec![BalanceSnapshot {
                asset: "USDT".to_string(),
                free: 1000.0,
                locked: 0.0,
            }],
            positions: vec![PositionSnapshot {
                instrument: Instrument::new("BTCUSDT"),
                market: Market::Futures,
                signed_qty: 0.25,
                entry_price: Some(65000.0),
            }],
            open_orders: vec![OpenOrder {
                order_id: None,
                client_order_id: "eth-order".to_string(),
                instrument: Instrument::new("ETHUSDT"),
                market: Market::Futures,
                side: Side::Sell,
                orig_qty: 1.0,
                executed_qty: 0.0,
                reduce_only: false,
                status: OrderStatus::Submitted,
            }],
        });

        let mut event_log = EventLog::default();
        log(
            &mut event_log,
            "app.execution.completed",
            json!({
                "command_kind": "set_target_exposure",
                "instrument": "SOLUSDT",
                "outcome_kind": "submitted",
            }),
        );

        let instruments = completion_instruments(&store, &event_log);

        assert_eq!(
            instruments,
            vec![
                "BTCUSDT".to_string(),
                "ETHUSDT".to_string(),
                "SOLUSDT".to_string(),
            ]
        );
    }

    #[test]
    fn prompt_status_uses_non_flat_positions_and_open_order_count() {
        let mut store = PortfolioStateStore::default();
        store.apply_snapshot(crate::exchange::types::AuthoritativeSnapshot {
            balances: vec![BalanceSnapshot {
                asset: "USDT".to_string(),
                free: 1000.0,
                locked: 0.0,
            }],
            positions: vec![
                PositionSnapshot {
                    instrument: Instrument::new("BTCUSDT"),
                    market: Market::Futures,
                    signed_qty: 0.25,
                    entry_price: Some(65000.0),
                },
                PositionSnapshot {
                    instrument: Instrument::new("ETHUSDT"),
                    market: Market::Futures,
                    signed_qty: 0.0,
                    entry_price: None,
                },
            ],
            open_orders: vec![OpenOrder {
                order_id: None,
                client_order_id: "btc-order".to_string(),
                instrument: Instrument::new("BTCUSDT"),
                market: Market::Futures,
                side: Side::Sell,
                orig_qty: 0.25,
                executed_qty: 0.0,
                reduce_only: false,
                status: OrderStatus::Submitted,
            }],
        });

        assert_eq!(prompt_status_from_store(&store), "[fresh|1 pos|1 ord]");
    }
}
