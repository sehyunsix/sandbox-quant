use std::sync::{Arc, Mutex};

use sandbox_quant::domain::instrument::Instrument;
use sandbox_quant::domain::market::Market;
use sandbox_quant::domain::order_type::OrderType;
use sandbox_quant::domain::position::Side;
use sandbox_quant::error::exchange_error::ExchangeError;
use sandbox_quant::exchange::binance::account::{RawAccountState, RawBalance, RawPosition};
use sandbox_quant::exchange::binance::client::{BinanceExchange, BinanceTransport};
use sandbox_quant::exchange::binance::orders::{
    RawCloseOrderAck, RawCloseOrderRequest, RawSymbolRules,
};
use sandbox_quant::exchange::facade::ExchangeFacade;
use sandbox_quant::exchange::types::CloseOrderRequest;

#[derive(Default)]
struct StubTransport {
    close_requests: Mutex<Vec<RawCloseOrderRequest>>,
}

impl StubTransport {
    fn close_requests(&self) -> Vec<RawCloseOrderRequest> {
        self.close_requests
            .lock()
            .expect("lock close_requests")
            .clone()
    }
}

impl BinanceTransport for StubTransport {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError> {
        match market {
            Market::Spot => Ok(RawAccountState {
                balances: vec![RawBalance {
                    asset: "USDT".to_string(),
                    free: 1000.0,
                    locked: 0.0,
                }],
                positions: vec![],
                open_orders: vec![],
            }),
            Market::Futures => Ok(RawAccountState {
                balances: vec![RawBalance {
                    asset: "USDT".to_string(),
                    free: 500.0,
                    locked: 0.0,
                }],
                positions: vec![RawPosition {
                    symbol: "BTCUSDT".to_string(),
                    signed_qty: -0.25,
                    entry_price: Some(65000.0),
                }],
                open_orders: vec![],
            }),
            Market::Options => Ok(RawAccountState {
                balances: vec![],
                positions: vec![],
                open_orders: vec![],
            }),
        }
    }

    fn load_last_price(&self, _symbol: &str, market: Market) -> Result<f64, ExchangeError> {
        match market {
            Market::Spot => Ok(50000.0),
            Market::Futures => Ok(65000.0),
            Market::Options => Ok(5.0),
        }
    }

    fn load_symbol_rules(
        &self,
        _symbol: &str,
        _market: Market,
    ) -> Result<RawSymbolRules, ExchangeError> {
        Ok(RawSymbolRules {
            min_qty: 0.001,
            max_qty: 100.0,
            step_size: 0.001,
        })
    }

    fn load_option_symbols(&self) -> Result<Vec<String>, ExchangeError> {
        Ok(vec!["BTC-260327-200000-C".to_string()])
    }

    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError> {
        self.close_requests
            .lock()
            .expect("lock close_requests")
            .push(request);
        Ok(RawCloseOrderAck {
            remote_order_id: "binance-close-1".to_string(),
        })
    }

    fn load_today_realized_pnl_usdt(&self) -> Result<f64, ExchangeError> {
        Ok(12.34)
    }

    fn load_today_funding_pnl_usdt(&self) -> Result<f64, ExchangeError> {
        Ok(5.67)
    }

    fn load_margin_ratio(&self) -> Result<Option<f64>, ExchangeError> {
        Ok(Some(0.1234))
    }
}

#[test]
fn binance_exchange_maps_spot_and_futures_account_state_into_authoritative_snapshot() {
    let exchange = BinanceExchange::new(Arc::new(StubTransport::default()));

    let snapshot = exchange
        .load_authoritative_snapshot()
        .expect("snapshot load should succeed");

    assert_eq!(snapshot.balances.len(), 2);
    assert_eq!(snapshot.positions.len(), 1);
    assert_eq!(snapshot.positions[0].instrument, Instrument::new("BTCUSDT"));
    assert_eq!(snapshot.positions[0].side(), Some(Side::Sell));
}

#[test]
fn binance_exchange_routes_close_submit_through_raw_transport_shape() {
    let transport = Arc::new(StubTransport::default());
    let exchange = BinanceExchange::new(transport.clone());

    let accepted = exchange
        .submit_close_order(CloseOrderRequest {
            instrument: Instrument::new("BTCUSDT"),
            market: Market::Futures,
            side: Side::Buy,
            qty: 0.25,
            qty_text: "0.25".to_string(),
            order_type: OrderType::Limit { price: 65000.0 },
            reduce_only: true,
        })
        .expect("close submit should succeed");

    assert_eq!(accepted.remote_order_id, "binance-close-1");
    let requests = transport.close_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].symbol, "BTCUSDT");
    assert_eq!(requests[0].side, "BUY");
    assert_eq!(requests[0].market, Market::Futures);
    assert_eq!(requests[0].qty, "0.25");
    assert_eq!(requests[0].order_type, OrderType::Limit { price: 65000.0 });
    assert!(requests[0].reduce_only);
}

#[test]
fn binance_exchange_loads_last_price_through_transport() {
    let exchange = BinanceExchange::new(Arc::new(StubTransport::default()));

    let price = exchange
        .load_last_price(&Instrument::new("BTCUSDT"), Market::Futures)
        .expect("last price should load");

    assert_eq!(price, 65000.0);
}
