use std::sync::Arc;

use sandbox_quant::binance::rest::BinanceRestClient;
use sandbox_quant::risk_module::{ApiEndpointGroup, EndpointRateLimits, RiskModule};

fn test_risk_module(global: u32, endpoint: EndpointRateLimits) -> RiskModule {
    let rest = Arc::new(BinanceRestClient::new(
        "https://demo-api.binance.com",
        "https://demo-fapi.binance.com",
        "k",
        "s",
        "fk",
        "fs",
        5000,
    ));
    RiskModule::new(rest, global, endpoint)
}

#[test]
/// Verifies global budget throttling for burst traffic:
/// once the configured per-minute limit is consumed, additional reservations fail.
fn global_rate_budget_blocks_after_limit() {
    let mut risk = test_risk_module(
        2,
        EndpointRateLimits {
            orders_per_minute: 10,
            account_per_minute: 10,
            market_data_per_minute: 10,
        },
    );

    assert!(risk.reserve_rate_budget());
    assert!(risk.reserve_rate_budget());
    assert!(!risk.reserve_rate_budget());
}

#[test]
/// Verifies endpoint-group isolation:
/// exhausting one endpoint group should not consume another group's budget.
fn endpoint_budgets_are_isolated_by_group() {
    let mut risk = test_risk_module(
        100,
        EndpointRateLimits {
            orders_per_minute: 1,
            account_per_minute: 2,
            market_data_per_minute: 3,
        },
    );

    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::Orders));
    assert!(!risk.reserve_endpoint_budget(ApiEndpointGroup::Orders));

    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::Account));
    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::Account));
    assert!(!risk.reserve_endpoint_budget(ApiEndpointGroup::Account));

    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::MarketData));
    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::MarketData));
    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::MarketData));
    assert!(!risk.reserve_endpoint_budget(ApiEndpointGroup::MarketData));
}

#[test]
/// Verifies endpoint budget snapshots:
/// used/limit should reflect consumed tokens for each endpoint group.
fn endpoint_budget_snapshot_reflects_usage() {
    let mut risk = test_risk_module(
        100,
        EndpointRateLimits {
            orders_per_minute: 5,
            account_per_minute: 5,
            market_data_per_minute: 5,
        },
    );

    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::Orders));
    assert!(risk.reserve_endpoint_budget(ApiEndpointGroup::Orders));
    let snap = risk.endpoint_budget_snapshot(ApiEndpointGroup::Orders);
    assert_eq!(snap.used, 2);
    assert_eq!(snap.limit, 5);
}
