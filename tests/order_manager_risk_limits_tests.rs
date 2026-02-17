use std::sync::Arc;

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::config::{
    EndpointRateLimitConfig, RiskConfig, SymbolExposureLimitConfig,
};
use sandbox_quant::model::order::OrderSide;
use sandbox_quant::order_manager::{MarketKind, OrderManager};
use sandbox_quant::risk_module::{EndpointRateLimits, RiskModule};

fn build_rest() -> Arc<BinanceRestClient> {
    Arc::new(BinanceRestClient::new(
        "https://demo-api.binance.com",
        "https://demo-fapi.binance.com",
        "k",
        "s",
        "fk",
        "fs",
        5000,
    ))
}

#[test]
/// Verifies symbol-level risk enforcement:
/// when projected notional exceeds configured symbol exposure limit, order must be rejected.
fn symbol_exposure_limit_prevents_order_acceptance() {
    let risk = RiskConfig {
        global_rate_limit_per_minute: 600,
        default_strategy_cooldown_ms: 0,
        default_strategy_max_active_orders: 10,
        default_symbol_max_exposure_usdt: 200.0,
        strategy_limits: vec![],
        symbol_exposure_limits: vec![SymbolExposureLimitConfig {
            symbol: "BTCUSDT".to_string(),
            market: Some("spot".to_string()),
            max_exposure_usdt: 150.0,
        }],
        endpoint_rate_limits: EndpointRateLimitConfig {
            orders_per_minute: 600,
            account_per_minute: 600,
            market_data_per_minute: 600,
        },
    };
    let mut mgr = OrderManager::new(build_rest(), "BTCUSDT", MarketKind::Spot, 10.0, &risk);
    mgr.update_unrealized_pnl(100.0);

    // 2 BTC at 100 USDT => 200 USDT projected exposure > 150 USDT limit.
    assert!(mgr.would_exceed_symbol_exposure_limit(OrderSide::Buy, 2.0));
}

#[test]
/// Verifies global-rate risk enforcement:
/// once global budget is depleted, additional intents cannot be accepted.
fn global_rate_budget_prevents_order_acceptance_after_exhaustion() {
    let rest = build_rest();
    let mut risk = RiskModule::new(
        rest,
        1,
        EndpointRateLimits {
            orders_per_minute: 10,
            account_per_minute: 10,
            market_data_per_minute: 10,
        },
    );

    assert!(risk.reserve_rate_budget());
    assert!(!risk.reserve_rate_budget());
}
