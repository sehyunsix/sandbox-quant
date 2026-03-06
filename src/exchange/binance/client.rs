use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::account::RawAccountState;
use crate::exchange::binance::auth::BinanceAuth;
use crate::exchange::binance::mapper::BinanceMapper;
use crate::exchange::binance::orders::{RawCloseOrderAck, RawCloseOrderRequest, RawSymbolRules};
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest, SubmitOrderAccepted,
};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::Value;

pub trait BinanceTransport: Send + Sync {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError>;
    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError>;
    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError>;
}

#[derive(Clone)]
pub struct BinanceExchange {
    transport: Arc<dyn BinanceTransport>,
    mapper: BinanceMapper,
}

impl BinanceExchange {
    pub fn new(transport: Arc<dyn BinanceTransport>) -> Self {
        Self {
            transport,
            mapper: BinanceMapper,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BinanceHttpTransport {
    client: Client,
    auth: BinanceAuth,
    spot_base_url: String,
    futures_base_url: String,
    recv_window_ms: u64,
}

impl BinanceHttpTransport {
    pub fn new(auth: BinanceAuth) -> Self {
        Self {
            client: Client::new(),
            auth,
            spot_base_url: "https://api.binance.com".to_string(),
            futures_base_url: "https://fapi.binance.com".to_string(),
            recv_window_ms: 5_000,
        }
    }

    pub fn with_base_urls(
        auth: BinanceAuth,
        spot_base_url: impl Into<String>,
        futures_base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            auth,
            spot_base_url: spot_base_url.into(),
            futures_base_url: futures_base_url.into(),
            recv_window_ms: 5_000,
        }
    }

    fn signed_get(&self, market: Market, path: &str) -> Result<Value, ExchangeError> {
        let query = self.auth.signed_query(&[
            ("timestamp", timestamp_millis().to_string()),
            ("recvWindow", self.recv_window_ms.to_string()),
        ]);
        let response = self
            .client
            .get(format!("{}{}?{}", self.base_url(market), path, query))
            .header("X-MBX-APIKEY", self.auth.api_key())
            .send()
            .map_err(map_reqwest_error)?;
        parse_json_response(response)
    }

    fn public_get(
        &self,
        market: Market,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, ExchangeError> {
        let url = if params.is_empty() {
            format!("{}{}", self.base_url(market), path)
        } else {
            let query = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
                .finish();
            format!("{}{}?{}", self.base_url(market), path, query)
        };
        let response = self.client.get(url).send().map_err(map_reqwest_error)?;
        parse_json_response(response)
    }

    fn signed_post(
        &self,
        market: Market,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, ExchangeError> {
        let mut signed_params: Vec<(&str, String)> = params.to_vec();
        signed_params.push(("timestamp", timestamp_millis().to_string()));
        signed_params.push(("recvWindow", self.recv_window_ms.to_string()));
        let body = self.auth.signed_query(&signed_params);
        let response = self
            .client
            .post(format!("{}{}", self.base_url(market), path))
            .header("X-MBX-APIKEY", self.auth.api_key())
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .map_err(map_reqwest_error)?;
        parse_json_response(response)
    }

    fn base_url(&self, market: Market) -> &str {
        match market {
            Market::Spot => &self.spot_base_url,
            Market::Futures => &self.futures_base_url,
        }
    }
}

impl BinanceTransport for BinanceHttpTransport {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError> {
        match market {
            Market::Spot => {
                let value = self.signed_get(Market::Spot, "/api/v3/account")?;
                parse_spot_account_state(value)
            }
            Market::Futures => {
                let account = self.signed_get(Market::Futures, "/fapi/v2/account")?;
                let positions = self.signed_get(Market::Futures, "/fapi/v2/positionRisk")?;
                parse_futures_account_state(account, positions)
            }
        }
    }

    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError> {
        let path = match market {
            Market::Spot => "/api/v3/exchangeInfo",
            Market::Futures => "/fapi/v1/exchangeInfo",
        };
        let value = self.public_get(market, path, &[("symbol", symbol.to_string())])?;
        parse_symbol_rules(value)
    }

    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError> {
        let mut params = vec![
            ("symbol", request.symbol),
            ("side", request.side.to_string()),
            ("type", "MARKET".to_string()),
            ("quantity", request.qty.to_string()),
            ("newOrderRespType", "ACK".to_string()),
        ];
        if request.market == Market::Futures && request.reduce_only {
            params.push(("reduceOnly", "true".to_string()));
        }
        let path = match request.market {
            Market::Spot => "/api/v3/order",
            Market::Futures => "/fapi/v1/order",
        };
        parse_order_ack(self.signed_post(request.market, path, &params)?)
    }
}

impl ExchangeFacade for BinanceExchange {
    type Error = ExchangeError;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error> {
        let mut spot = self
            .mapper
            .map_account_snapshot(Market::Spot, self.transport.load_account_state(Market::Spot)?);
        let futures = self.mapper.map_account_snapshot(
            Market::Futures,
            self.transport.load_account_state(Market::Futures)?,
        );
        spot.positions.extend(futures.positions);
        spot.balances.extend(futures.balances);
        Ok(spot)
    }

    fn load_symbol_rules(
        &self,
        instrument: &Instrument,
        market: Market,
    ) -> Result<SymbolRules, Self::Error> {
        let rules = self.transport.load_symbol_rules(&instrument.0, market)?;
        Ok(self.mapper.map_symbol_rules(rules))
    }

    fn submit_close_order(
        &self,
        request: CloseOrderRequest,
    ) -> Result<CloseOrderAccepted, Self::Error> {
        let raw = self.mapper.map_close_request(request);
        let ack = self.transport.submit_close_order(raw)?;
        Ok(self.mapper.map_close_ack(ack))
    }

    fn submit_order(&self, request: CloseOrderRequest) -> Result<SubmitOrderAccepted, Self::Error> {
        let raw = self.mapper.map_close_request(request);
        let ack = self.transport.submit_close_order(raw)?;
        Ok(SubmitOrderAccepted {
            remote_order_id: ack.remote_order_id,
        })
    }
}

#[derive(Debug, Deserialize)]
struct BinanceErrorBody {
    code: i64,
    msg: String,
}

pub fn map_binance_http_error(status: u16, body: &str) -> ExchangeError {
    if status == 429 || status == 418 {
        return ExchangeError::RateLimited;
    }
    if status == 401 || status == 403 {
        return ExchangeError::AuthenticationFailed;
    }
    if let Ok(error) = serde_json::from_str::<BinanceErrorBody>(body) {
        return match error.code {
            -1021 => ExchangeError::InvalidTimestamp,
            -2014 | -2015 => ExchangeError::AuthenticationFailed,
            _ => ExchangeError::RemoteReject {
                code: error.code,
                message: error.msg,
            },
        };
    }
    ExchangeError::InvalidResponse
}

fn map_reqwest_error(error: reqwest::Error) -> ExchangeError {
    if error.is_timeout() {
        ExchangeError::NetworkTimeout
    } else {
        ExchangeError::TransportFailure
    }
}

fn parse_json_response(response: Response) -> Result<Value, ExchangeError> {
    let status = response.status();
    let body = response.text().map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(map_binance_http_error(status.as_u16(), &body));
    }
    serde_json::from_str(&body).map_err(|_| ExchangeError::InvalidResponse)
}

fn parse_spot_account_state(value: Value) -> Result<RawAccountState, ExchangeError> {
    #[derive(Deserialize)]
    struct SpotAccount {
        balances: Vec<SpotBalance>,
    }
    #[derive(Deserialize)]
    struct SpotBalance {
        asset: String,
        free: String,
        locked: String,
    }

    let account: SpotAccount =
        serde_json::from_value(value).map_err(|_| ExchangeError::InvalidResponse)?;
    let balances = account
        .balances
        .into_iter()
        .map(|balance| {
            Ok(crate::exchange::binance::account::RawBalance {
                asset: balance.asset,
                free: parse_decimal(&balance.free)?,
                locked: parse_decimal(&balance.locked)?,
            })
        })
        .collect::<Result<Vec<_>, ExchangeError>>()?;

    Ok(RawAccountState {
        balances,
        positions: Vec::new(),
    })
}

fn parse_futures_account_state(
    account_value: Value,
    positions_value: Value,
) -> Result<RawAccountState, ExchangeError> {
    #[derive(Deserialize)]
    struct FuturesAccount {
        assets: Vec<FuturesAsset>,
    }
    #[derive(Deserialize)]
    struct FuturesAsset {
        asset: String,
        #[serde(rename = "availableBalance")]
        available_balance: String,
        #[serde(rename = "walletBalance")]
        wallet_balance: String,
    }
    #[derive(Deserialize)]
    struct FuturesPosition {
        symbol: String,
        #[serde(rename = "positionAmt")]
        position_amt: String,
        #[serde(rename = "entryPrice")]
        entry_price: String,
    }

    let account: FuturesAccount =
        serde_json::from_value(account_value).map_err(|_| ExchangeError::InvalidResponse)?;
    let positions: Vec<FuturesPosition> =
        serde_json::from_value(positions_value).map_err(|_| ExchangeError::InvalidResponse)?;

    let balances = account
        .assets
        .into_iter()
        .map(|asset| {
            let free = parse_decimal(&asset.available_balance)?;
            let wallet = parse_decimal(&asset.wallet_balance)?;
            Ok(crate::exchange::binance::account::RawBalance {
                asset: asset.asset,
                free,
                locked: (wallet - free).max(0.0),
            })
        })
        .collect::<Result<Vec<_>, ExchangeError>>()?;

    let positions = positions
        .into_iter()
        .map(|position| {
            let signed_qty = parse_decimal(&position.position_amt)?;
            let entry_price = parse_decimal(&position.entry_price)?;
            Ok(crate::exchange::binance::account::RawPosition {
                symbol: position.symbol,
                signed_qty,
                entry_price: if entry_price.abs() <= f64::EPSILON {
                    None
                } else {
                    Some(entry_price)
                },
            })
        })
        .collect::<Result<Vec<_>, ExchangeError>>()?;

    Ok(RawAccountState { balances, positions })
}

fn parse_symbol_rules(value: Value) -> Result<RawSymbolRules, ExchangeError> {
    let symbol = value["symbols"]
        .as_array()
        .and_then(|symbols| symbols.first())
        .ok_or(ExchangeError::InvalidResponse)?;
    let filters = symbol["filters"]
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?;
    let lot_size = filters
        .iter()
        .find(|filter| filter["filterType"].as_str() == Some("LOT_SIZE"))
        .ok_or(ExchangeError::InvalidResponse)?;

    Ok(RawSymbolRules {
        min_qty: parse_decimal(lot_size["minQty"].as_str().ok_or(ExchangeError::InvalidResponse)?)?,
        max_qty: parse_decimal(lot_size["maxQty"].as_str().ok_or(ExchangeError::InvalidResponse)?)?,
        step_size: parse_decimal(
            lot_size["stepSize"]
                .as_str()
                .ok_or(ExchangeError::InvalidResponse)?,
        )?,
    })
}

fn parse_order_ack(value: Value) -> Result<RawCloseOrderAck, ExchangeError> {
    let remote_order_id = value["orderId"]
        .as_i64()
        .map(|id| id.to_string())
        .or_else(|| value["clientOrderId"].as_str().map(str::to_string))
        .ok_or(ExchangeError::InvalidResponse)?;
    Ok(RawCloseOrderAck { remote_order_id })
}

fn parse_decimal(raw: &str) -> Result<f64, ExchangeError> {
    raw.parse::<f64>().map_err(|_| ExchangeError::InvalidResponse)
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis()
}
