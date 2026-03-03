use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::event::{AppEvent, AssetPnlEntry};
use crate::order_manager::OrderManager;
use crate::runtime::portfolio_layer_state::build_portfolio_layer_state;

pub async fn emit_portfolio_state_updates(
    app_tx: &mpsc::Sender<AppEvent>,
    order_managers: &HashMap<String, OrderManager>,
    realized_pnl_by_symbol: &HashMap<String, f64>,
    live_futures_positions: &HashMap<String, AssetPnlEntry>,
) {
    let portfolio_state = build_portfolio_layer_state(
        order_managers,
        realized_pnl_by_symbol,
        live_futures_positions,
    );
    let _ = app_tx
        .send(AppEvent::PortfolioStateUpdate {
            snapshot: portfolio_state.to_snapshot(),
        })
        .await;
}
