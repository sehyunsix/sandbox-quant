use std::sync::Arc;

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::config::{EndpointRateLimitConfig, RiskConfig, SymbolExposureLimitConfig};
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

#[tokio::test]
/// Verifies protective-stop helper behavior in flat state:
/// no position should require no stop placement.
async fn ensure_protective_stop_returns_true_when_flat() {
    let risk = RiskConfig {
        global_rate_limit_per_minute: 600,
        default_strategy_cooldown_ms: 0,
        default_strategy_max_active_orders: 10,
        default_symbol_max_exposure_usdt: 200.0,
        strategy_limits: vec![],
        symbol_exposure_limits: vec![],
        endpoint_rate_limits: EndpointRateLimitConfig {
            orders_per_minute: 600,
            account_per_minute: 600,
            market_data_per_minute: 600,
        },
    };
    let mut mgr = OrderManager::new(build_rest(), "BTCUSDT", MarketKind::Spot, 10.0, &risk);
    let ok = mgr
        .ensure_protective_stop("sys", 90.0)
        .await
        .expect("ensure should succeed");
    assert!(ok);
}

#[tokio::test]
/// Verifies emergency-close helper behavior in flat state:
/// should be a no-op and avoid broker calls.
async fn emergency_close_returns_none_when_flat() {
    let risk = RiskConfig {
        global_rate_limit_per_minute: 600,
        default_strategy_cooldown_ms: 0,
        default_strategy_max_active_orders: 10,
        default_symbol_max_exposure_usdt: 200.0,
        strategy_limits: vec![],
        symbol_exposure_limits: vec![],
        endpoint_rate_limits: EndpointRateLimitConfig {
            orders_per_minute: 600,
            account_per_minute: 600,
            market_data_per_minute: 600,
        },
    };
    let mut mgr = OrderManager::new(build_rest(), "BTCUSDT", MarketKind::Spot, 10.0, &risk);
    let out = mgr
        .emergency_close_position("sys", "exit.emergency_close")
        .await
        .expect("emergency close should succeed");
    assert!(out.is_none());
}
