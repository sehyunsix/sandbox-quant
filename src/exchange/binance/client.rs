use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::instrument::Instrument;
use crate::domain::market::Market;
use crate::error::exchange_error::ExchangeError;
use crate::exchange::binance::account::RawAccountState;
use crate::exchange::binance::auth::BinanceAuth;
use crate::exchange::binance::mapper::BinanceMapper;
use crate::exchange::binance::orders::{
    RawCloseOrderAck, RawCloseOrderRequest, RawOpenOrder, RawSymbolRules,
};
use crate::exchange::facade::ExchangeFacade;
use crate::exchange::symbol_rules::SymbolRules;
use crate::exchange::types::{
    AuthoritativeSnapshot, CloseOrderAccepted, CloseOrderRequest, SubmitOrderAccepted,
};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::Value;

pub trait BinanceTransport: Send + Sync {
    fn transport_name(&self) -> &'static str {
        "real"
    }

    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError>;
    fn load_last_price(&self, symbol: &str, market: Market) -> Result<f64, ExchangeError>;
    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError>;
    fn load_option_symbols(&self) -> Result<Vec<String>, ExchangeError>;
    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError>;
    fn load_today_realized_pnl_usdt(&self) -> Result<f64, ExchangeError>;
    fn load_today_funding_pnl_usdt(&self) -> Result<f64, ExchangeError>;
    fn load_margin_ratio(&self) -> Result<Option<f64>, ExchangeError>;
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

    pub fn transport_name(&self) -> &'static str {
        self.transport.transport_name()
    }

    pub fn load_option_symbols(&self) -> Result<Vec<String>, ExchangeError> {
        self.transport.load_option_symbols()
    }
}

#[derive(Debug, Clone)]
pub struct BinanceHttpTransport {
    client: Client,
    auth: BinanceAuth,
    spot_base_url: String,
    futures_base_url: String,
    options_base_url: String,
    recv_window_ms: u64,
}

impl BinanceHttpTransport {
    pub fn new(auth: BinanceAuth) -> Self {
        Self {
            client: Client::new(),
            auth,
            spot_base_url: "https://api.binance.com".to_string(),
            futures_base_url: "https://fapi.binance.com".to_string(),
            options_base_url: "https://eapi.binance.com".to_string(),
            recv_window_ms: 5_000,
        }
    }

    pub fn with_base_urls(
        auth: BinanceAuth,
        spot_base_url: impl Into<String>,
        futures_base_url: impl Into<String>,
        options_base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            auth,
            spot_base_url: spot_base_url.into(),
            futures_base_url: futures_base_url.into(),
            options_base_url: options_base_url.into(),
            recv_window_ms: 5_000,
        }
    }

    fn signed_get(
        &self,
        market: Market,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<Value, ExchangeError> {
        let mut signed_params: Vec<(&str, String)> = params.to_vec();
        signed_params.push(("timestamp", timestamp_millis().to_string()));
        signed_params.push(("recvWindow", self.recv_window_ms.to_string()));
        let query = self.auth.signed_query(&signed_params);
        let response = self
            .client
            .get(format!("{}{}?{}", self.base_url(market), path, query))
            .header("X-MBX-APIKEY", self.auth.api_key())
            .send()
            .map_err(map_reqwest_error)?;
        parse_json_response(response, path)
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
        parse_json_response(response, path)
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
        parse_json_response(response, path)
    }

    fn base_url(&self, market: Market) -> &str {
        match market {
            Market::Spot => &self.spot_base_url,
            Market::Futures => &self.futures_base_url,
            Market::Options => &self.options_base_url,
        }
    }
}

impl BinanceTransport for BinanceHttpTransport {
    fn load_account_state(&self, market: Market) -> Result<RawAccountState, ExchangeError> {
        match market {
            Market::Spot => {
                let value = self.signed_get(Market::Spot, "/api/v3/account", &[])?;
                parse_spot_account_state(value)
            }
            Market::Futures => {
                let account = self.signed_get(Market::Futures, "/fapi/v2/account", &[])?;
                let positions = self.signed_get(Market::Futures, "/fapi/v2/positionRisk", &[])?;
                parse_futures_account_state(account, positions)
            }
            Market::Options => {
                let account = self.signed_get(Market::Options, "/eapi/v1/marginAccount", &[])?;
                let positions = self.signed_get(Market::Options, "/eapi/v1/position", &[])?;
                let open_orders = self.signed_get(Market::Options, "/eapi/v1/openOrders", &[])?;
                parse_options_account_state(account, positions, open_orders)
            }
        }
    }

    fn load_last_price(&self, symbol: &str, market: Market) -> Result<f64, ExchangeError> {
        let path = match market {
            Market::Spot => "/api/v3/ticker/price",
            Market::Futures => "/fapi/v1/ticker/price",
            Market::Options => "/eapi/v1/ticker",
        };
        let value = self.public_get(market, path, &[("symbol", symbol.to_string())])?;
        parse_last_price(value, market)
    }

    fn load_symbol_rules(
        &self,
        symbol: &str,
        market: Market,
    ) -> Result<RawSymbolRules, ExchangeError> {
        let path = match market {
            Market::Spot => "/api/v3/exchangeInfo",
            Market::Futures => "/fapi/v1/exchangeInfo",
            Market::Options => "/eapi/v1/exchangeInfo",
        };
        let value = self.public_get(market, path, &[("symbol", symbol.to_string())])?;
        parse_symbol_rules(value, market)
    }

    fn load_option_symbols(&self) -> Result<Vec<String>, ExchangeError> {
        let value = self.public_get(Market::Options, "/eapi/v1/exchangeInfo", &[])?;
        parse_option_symbols(value)
    }

    fn submit_close_order(
        &self,
        request: RawCloseOrderRequest,
    ) -> Result<RawCloseOrderAck, ExchangeError> {
        let mut params = vec![
            ("symbol", request.symbol),
            ("side", request.side.to_string()),
            ("quantity", request.qty),
            ("newOrderRespType", "ACK".to_string()),
        ];
        match request.order_type {
            crate::domain::order_type::OrderType::Market => {
                params.push(("type", "MARKET".to_string()));
            }
            crate::domain::order_type::OrderType::Limit { price } => {
                params.push(("type", "LIMIT".to_string()));
                params.push(("timeInForce", "GTC".to_string()));
                params.push(("price", price.to_string()));
            }
        }
        if request.market == Market::Futures && request.reduce_only {
            params.push(("reduceOnly", "true".to_string()));
        }
        let path = match request.market {
            Market::Spot => "/api/v3/order",
            Market::Futures => "/fapi/v1/order",
            Market::Options => "/eapi/v1/order",
        };
        parse_order_ack(self.signed_post(request.market, path, &params)?)
    }

    fn load_today_realized_pnl_usdt(&self) -> Result<f64, ExchangeError> {
        load_income_total(self, "REALIZED_PNL")
    }

    fn load_today_funding_pnl_usdt(&self) -> Result<f64, ExchangeError> {
        load_income_total(self, "FUNDING_FEE")
    }

    fn load_margin_ratio(&self) -> Result<Option<f64>, ExchangeError> {
        let account = self.signed_get(Market::Futures, "/fapi/v2/account", &[])?;
        parse_margin_ratio(account)
    }
}

impl ExchangeFacade for BinanceExchange {
    type Error = ExchangeError;

    fn load_authoritative_snapshot(&self) -> Result<AuthoritativeSnapshot, Self::Error> {
        let mut spot = self.mapper.map_account_snapshot(
            Market::Spot,
            self.transport.load_account_state(Market::Spot)?,
        );
        let futures = self.mapper.map_account_snapshot(
            Market::Futures,
            self.transport.load_account_state(Market::Futures)?,
        );
        if let Ok(options) = self.transport.load_account_state(Market::Options) {
            let options = self.mapper.map_account_snapshot(Market::Options, options);
            spot.positions.extend(options.positions);
            spot.balances.extend(options.balances);
            spot.open_orders.extend(options.open_orders);
        }
        spot.positions.extend(futures.positions);
        spot.balances.extend(futures.balances);
        spot.open_orders.extend(futures.open_orders);
        Ok(spot)
    }

    fn load_today_realized_pnl_usdt(&self) -> Result<f64, Self::Error> {
        self.transport.load_today_realized_pnl_usdt()
    }

    fn load_today_funding_pnl_usdt(&self) -> Result<f64, Self::Error> {
        self.transport.load_today_funding_pnl_usdt()
    }

    fn load_margin_ratio(&self) -> Result<Option<f64>, Self::Error> {
        self.transport.load_margin_ratio()
    }

    fn load_last_price(&self, instrument: &Instrument, market: Market) -> Result<f64, Self::Error> {
        self.transport.load_last_price(&instrument.0, market)
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

pub fn map_binance_http_error(status: u16, body: &str, endpoint: &str) -> ExchangeError {
    if status == 429 || status == 418 {
        let (code, message) = parse_error_body(body)
            .map(|error| (Some(error.code), error.msg))
            .unwrap_or((None, body.to_string()));
        return ExchangeError::RateLimited {
            status,
            code,
            endpoint: endpoint.to_string(),
            message,
        };
    }
    if status == 401 || status == 403 {
        let (code, message) = parse_error_body(body)
            .map(|error| (Some(error.code), error.msg))
            .unwrap_or((None, body.to_string()));
        return ExchangeError::AuthenticationFailed {
            status,
            code,
            endpoint: endpoint.to_string(),
            message,
        };
    }
    if let Some(error) = parse_error_body(body) {
        return match error.code {
            -1021 => ExchangeError::InvalidTimestamp,
            -2014 | -2015 => ExchangeError::AuthenticationFailed {
                status,
                code: Some(error.code),
                endpoint: endpoint.to_string(),
                message: error.msg,
            },
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

fn parse_json_response(response: Response, endpoint: &str) -> Result<Value, ExchangeError> {
    let status = response.status();
    let body = response.text().map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(map_binance_http_error(status.as_u16(), &body, endpoint));
    }
    serde_json::from_str(&body).map_err(|_| ExchangeError::InvalidResponse)
}

fn parse_error_body(body: &str) -> Option<BinanceErrorBody> {
    serde_json::from_str::<BinanceErrorBody>(body).ok()
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
        open_orders: Vec::new(),
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

    Ok(RawAccountState {
        balances,
        positions,
        open_orders: Vec::new(),
    })
}

fn parse_options_account_state(
    account_value: Value,
    positions_value: Value,
    open_orders_value: Value,
) -> Result<RawAccountState, ExchangeError> {
    let balances = account_value["asset"]
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?
        .iter()
        .map(|asset| {
            let asset_name = asset["asset"]
                .as_str()
                .ok_or(ExchangeError::InvalidResponse)?
                .to_string();
            let free = parse_decimal_value(&asset["available"])?;
            let margin_balance = parse_decimal_value(&asset["marginBalance"])?;
            Ok(crate::exchange::binance::account::RawBalance {
                asset: asset_name,
                free,
                locked: (margin_balance - free).max(0.0),
            })
        })
        .collect::<Result<Vec<_>, ExchangeError>>()?;

    let positions = positions_value
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?
        .iter()
        .filter_map(parse_options_position)
        .collect::<Result<Vec<_>, ExchangeError>>()?;

    let open_orders = parse_options_open_orders(open_orders_value)?;

    Ok(RawAccountState {
        balances,
        positions,
        open_orders,
    })
}

fn parse_symbol_rules(value: Value, market: Market) -> Result<RawSymbolRules, ExchangeError> {
    let symbol = match market {
        Market::Options => value["optionSymbols"]
            .as_array()
            .and_then(|symbols| symbols.first()),
        _ => value["symbols"]
            .as_array()
            .and_then(|symbols| symbols.first()),
    }
    .ok_or(ExchangeError::InvalidResponse)?;
    let filters = symbol["filters"]
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?;
    let lot_size = filters
        .iter()
        .find(|filter| filter["filterType"].as_str() == Some("LOT_SIZE"))
        .ok_or(ExchangeError::InvalidResponse)?;

    Ok(RawSymbolRules {
        min_qty: parse_decimal(
            lot_size["minQty"]
                .as_str()
                .ok_or(ExchangeError::InvalidResponse)?,
        )?,
        max_qty: parse_decimal(
            lot_size["maxQty"]
                .as_str()
                .ok_or(ExchangeError::InvalidResponse)?,
        )?,
        step_size: parse_decimal(
            lot_size["stepSize"]
                .as_str()
                .ok_or(ExchangeError::InvalidResponse)?,
        )?,
    })
}

fn parse_option_symbols(value: Value) -> Result<Vec<String>, ExchangeError> {
    Ok(value["optionSymbols"]
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?
        .iter()
        .filter_map(|item| item["symbol"].as_str().map(str::to_string))
        .collect::<Vec<_>>())
}

fn parse_last_price(value: Value, market: Market) -> Result<f64, ExchangeError> {
    let price = match market {
        Market::Options => value
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["lastPrice"].as_str()),
        _ => value["price"].as_str(),
    }
    .ok_or(ExchangeError::InvalidResponse)?;
    parse_decimal(price)
}

fn parse_options_position(
    item: &Value,
) -> Option<Result<crate::exchange::binance::account::RawPosition, ExchangeError>> {
    let symbol = item["symbol"].as_str()?.to_string();
    let qty_value = match item
        .get("quantity")
        .or_else(|| item.get("positionQty"))
        .or_else(|| item.get("qty"))
    {
        Some(value) => value,
        None => return Some(Err(ExchangeError::InvalidResponse)),
    };
    let raw_qty = match parse_decimal_value(qty_value) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };

    if raw_qty.abs() <= f64::EPSILON {
        return None;
    }

    let side = item
        .get("side")
        .and_then(Value::as_str)
        .or_else(|| item.get("positionSide").and_then(Value::as_str));
    let signed_qty = match side {
        Some("SHORT") | Some("SELL") => -raw_qty.abs(),
        Some("LONG") | Some("BUY") => raw_qty.abs(),
        _ => raw_qty,
    };

    let entry_price = item
        .get("entryPrice")
        .or_else(|| item.get("avgPrice"))
        .or_else(|| item.get("markPrice"))
        .and_then(|value| parse_decimal_value(value).ok())
        .filter(|value| value.abs() > f64::EPSILON);

    Some(Ok(crate::exchange::binance::account::RawPosition {
        symbol,
        signed_qty,
        entry_price,
    }))
}

fn parse_options_open_orders(value: Value) -> Result<Vec<RawOpenOrder>, ExchangeError> {
    value
        .as_array()
        .ok_or(ExchangeError::InvalidResponse)?
        .iter()
        .map(|item| {
            Ok(RawOpenOrder {
                order_id: item["orderId"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .or_else(|| item["orderId"].as_str().map(str::to_string)),
                client_order_id: item["clientOrderId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                symbol: item["symbol"]
                    .as_str()
                    .ok_or(ExchangeError::InvalidResponse)?
                    .to_string(),
                market: Market::Options,
                side: match item["side"].as_str().unwrap_or("BUY") {
                    "SELL" => "SELL",
                    _ => "BUY",
                },
                orig_qty: parse_decimal_value(&item["quantity"])?,
                executed_qty: parse_decimal_value(&item["executedQty"])?,
                reduce_only: item["reduceOnly"].as_bool().unwrap_or(false),
                status: item["status"].as_str().unwrap_or("NEW").to_string(),
            })
        })
        .collect()
}

fn parse_order_ack(value: Value) -> Result<RawCloseOrderAck, ExchangeError> {
    let remote_order_id = value["orderId"]
        .as_i64()
        .map(|id| id.to_string())
        .or_else(|| value["clientOrderId"].as_str().map(str::to_string))
        .ok_or(ExchangeError::InvalidResponse)?;
    Ok(RawCloseOrderAck { remote_order_id })
}

fn parse_income_total(value: Value) -> Result<f64, ExchangeError> {
    let incomes = value.as_array().ok_or(ExchangeError::InvalidResponse)?;
    incomes.iter().try_fold(0.0, |acc, item| {
        let income = item["income"]
            .as_str()
            .ok_or(ExchangeError::InvalidResponse)?;
        Ok(acc + parse_decimal(income)?)
    })
}

fn load_income_total(
    transport: &BinanceHttpTransport,
    income_type: &str,
) -> Result<f64, ExchangeError> {
    let start_time = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid start of day")
        .and_local_timezone(chrono::Local)
        .single()
        .expect("single local datetime")
        .timestamp_millis();
    let value = transport.signed_get(
        Market::Futures,
        "/fapi/v1/income",
        &[
            ("incomeType", income_type.to_string()),
            ("startTime", start_time.to_string()),
            ("limit", "1000".to_string()),
        ],
    )?;
    parse_income_total(value)
}

fn parse_margin_ratio(value: Value) -> Result<Option<f64>, ExchangeError> {
    let total_maint_margin = value["totalMaintMargin"]
        .as_str()
        .ok_or(ExchangeError::InvalidResponse)
        .and_then(parse_decimal)?;
    let total_margin_balance = value["totalMarginBalance"]
        .as_str()
        .ok_or(ExchangeError::InvalidResponse)
        .and_then(parse_decimal)?;

    if total_margin_balance <= f64::EPSILON {
        return Ok(None);
    }

    Ok(Some(total_maint_margin / total_margin_balance))
}

fn parse_decimal(raw: &str) -> Result<f64, ExchangeError> {
    raw.parse::<f64>()
        .map_err(|_| ExchangeError::InvalidResponse)
}

fn parse_decimal_value(value: &Value) -> Result<f64, ExchangeError> {
    if let Some(raw) = value.as_str() {
        return parse_decimal(raw);
    }
    if let Some(raw) = value.as_f64() {
        return Ok(raw);
    }
    Err(ExchangeError::InvalidResponse)
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis()
}
