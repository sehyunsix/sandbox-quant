use std::collections::HashMap;

use super::AppState;

#[derive(Debug, Clone, Default)]
pub struct PortfolioSummary {
    pub total_equity_usdt: Option<f64>,
    pub total_realized_pnl_usdt: f64,
    pub total_unrealized_pnl_usdt: f64,
    pub ws_connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AssetEntry {
    pub symbol: String,
    pub last_price: Option<f64>,
    pub position_qty: f64,
    pub realized_pnl_usdt: f64,
    pub unrealized_pnl_usdt: f64,
}

#[derive(Debug, Clone, Default)]
pub struct StrategyEntry {
    pub strategy_id: String,
    pub trade_count: u32,
    pub win_count: u32,
    pub lose_count: u32,
    pub realized_pnl_usdt: f64,
}

#[derive(Debug, Clone, Default)]
pub struct MatrixCell {
    pub symbol: String,
    pub strategy_id: String,
    pub trade_count: u32,
    pub realized_pnl_usdt: f64,
}

#[derive(Debug, Clone, Default)]
pub struct FocusState {
    pub symbol: Option<String>,
    pub strategy_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UiProjection {
    pub portfolio: PortfolioSummary,
    pub assets: Vec<AssetEntry>,
    pub strategies: Vec<StrategyEntry>,
    pub matrix: Vec<MatrixCell>,
    pub focus: FocusState,
}

impl UiProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_legacy(state: &AppState) -> Self {
        let mut strategy_rows = Vec::new();
        let mut matrix_rows = Vec::new();
        for (strategy_id, stats) in &state.strategy_stats {
            strategy_rows.push(StrategyEntry {
                strategy_id: strategy_id.clone(),
                trade_count: stats.trade_count,
                win_count: stats.win_count,
                lose_count: stats.lose_count,
                realized_pnl_usdt: stats.realized_pnl,
            });
            matrix_rows.push(MatrixCell {
                symbol: state.symbol.clone(),
                strategy_id: strategy_id.clone(),
                trade_count: stats.trade_count,
                realized_pnl_usdt: stats.realized_pnl,
            });
        }
        strategy_rows.sort_by(|a, b| a.strategy_id.cmp(&b.strategy_id));
        matrix_rows.sort_by(|a, b| {
            a.symbol
                .cmp(&b.symbol)
                .then_with(|| a.strategy_id.cmp(&b.strategy_id))
        });
        let mut asset_symbols: Vec<String> = state
            .symbol_items
            .iter()
            .cloned()
            .chain(state.strategy_item_symbols.iter().cloned())
            .chain(state.balances.keys().cloned())
            .filter(|s| !s.trim().is_empty())
            .collect();
        asset_symbols.sort();
        asset_symbols.dedup();
        if asset_symbols.is_empty() {
            asset_symbols.push(state.symbol.clone());
        }
        let assets = asset_symbols
            .into_iter()
            .map(|symbol| {
                if symbol == state.symbol {
                    AssetEntry {
                        symbol,
                        last_price: state.last_price(),
                        position_qty: state.position.qty,
                        realized_pnl_usdt: state.history_realized_pnl,
                        unrealized_pnl_usdt: state.position.unrealized_pnl,
                    }
                } else {
                    let inferred_qty = state
                        .balances
                        .get(&symbol)
                        .copied()
                        .or_else(|| {
                            let (base, _) = split_symbol_assets(&symbol);
                            if base.is_empty() {
                                None
                            } else {
                                state.balances.get(&base).copied()
                            }
                        })
                        .unwrap_or(0.0);
                    AssetEntry {
                        symbol,
                        last_price: None,
                        position_qty: inferred_qty,
                        realized_pnl_usdt: 0.0,
                        unrealized_pnl_usdt: 0.0,
                    }
                }
            })
            .collect();

        Self {
            portfolio: PortfolioSummary {
                total_equity_usdt: state.current_equity_usdt,
                total_realized_pnl_usdt: state.history_realized_pnl,
                total_unrealized_pnl_usdt: state.position.unrealized_pnl,
                ws_connected: state.ws_connected,
            },
            assets,
            strategies: strategy_rows,
            matrix: matrix_rows,
            focus: FocusState {
                symbol: Some(state.symbol.clone()),
                strategy_id: Some(state.strategy_label.clone()),
            },
        }
    }

    pub fn strategy_lookup(&self) -> HashMap<String, StrategyEntry> {
        self.strategies
            .iter()
            .cloned()
            .map(|s| (s.strategy_id.clone(), s))
            .collect()
    }
}

fn split_symbol_assets(symbol: &str) -> (String, String) {
    const QUOTE_SUFFIXES: [&str; 10] = [
        "USDT", "USDC", "FDUSD", "BUSD", "TUSD", "TRY", "EUR", "BTC", "ETH", "BNB",
    ];
    for q in QUOTE_SUFFIXES {
        if let Some(base) = symbol.strip_suffix(q) {
            if !base.is_empty() {
                return (base.to_string(), q.to_string());
            }
        }
    }
    (String::new(), String::new())
}
