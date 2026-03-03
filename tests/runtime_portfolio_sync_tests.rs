use sandbox_quant::binance::types::BinanceFuturesPositionRisk;
use sandbox_quant::runtime::portfolio_sync::build_live_futures_positions;

fn norm(symbol: &str) -> String {
    format!("{} (FUT)", symbol)
}

#[test]
fn skips_zero_qty_rows() {
    let rows = vec![BinanceFuturesPositionRisk {
        symbol: "BTCUSDT".to_string(),
        position_amt: 0.0,
        entry_price: 100.0,
        mark_price: 101.0,
        unrealized_profit: 1.0,
    }];
    let out = build_live_futures_positions(&rows, norm);
    assert!(out.is_empty());
}

#[test]
fn maps_long_and_short_with_side_and_qty() {
    let rows = vec![
        BinanceFuturesPositionRisk {
            symbol: "BTCUSDT".to_string(),
            position_amt: 0.3,
            entry_price: 100.0,
            mark_price: 101.0,
            unrealized_profit: 0.0,
        },
        BinanceFuturesPositionRisk {
            symbol: "ETHUSDT".to_string(),
            position_amt: -0.2,
            entry_price: 200.0,
            mark_price: 199.0,
            unrealized_profit: 0.0,
        },
    ];
    let out = build_live_futures_positions(&rows, norm);
    let btc = out.get("BTCUSDT (FUT)").expect("btc row");
    let eth = out.get("ETHUSDT (FUT)").expect("eth row");
    assert_eq!(btc.position_qty, 0.3);
    assert_eq!(eth.position_qty, 0.2);
    assert!(btc.side.is_some());
    assert!(eth.side.is_some());
}
