use serde::Deserialize;

/// Deserialize Binance string-encoded numbers to f64.
pub fn string_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<f64>().map_err(serde::de::Error::custom)
}

pub fn string_or_number_to_f64_default<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(0.0),
        serde_json::Value::String(s) => s.parse::<f64>().map_err(serde::de::Error::custom),
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| serde::de::Error::custom("invalid number")),
        _ => Err(serde::de::Error::custom("invalid numeric value")),
    }
}

/// Binance trade stream event (symbol@trade).
#[derive(Debug, Deserialize)]
pub struct BinanceTradeEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub trade_id: u64,
    #[serde(rename = "p", deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(rename = "q", deserialize_with = "string_to_f64")]
    pub qty: f64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Binance order response (newOrderRespType=FULL).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderResponse {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    #[serde(deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub executed_qty: f64,
    pub status: String,
    pub r#type: String,
    pub side: String,
    #[serde(default)]
    pub fills: Vec<BinanceFill>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFill {
    #[serde(deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub qty: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub commission: f64,
    pub commission_asset: String,
}

/// Binance all orders response item (GET /api/v3/allOrders).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceAllOrder {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    #[serde(deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub executed_qty: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub cummulative_quote_qty: f64,
    pub status: String,
    pub r#type: String,
    pub side: String,
    pub time: u64,
    pub update_time: u64,
}

/// Binance my trades response item (GET /api/v3/myTrades).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceMyTrade {
    pub symbol: String,
    pub id: u64,
    pub order_id: u64,
    #[serde(deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub qty: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub commission: f64,
    pub commission_asset: String,
    pub time: u64,
    pub is_buyer: bool,
    pub is_maker: bool,
    #[serde(default, deserialize_with = "string_or_number_to_f64_default")]
    pub realized_pnl: f64,
}

/// Binance API error response.
#[derive(Debug, Deserialize)]
pub struct BinanceApiErrorResponse {
    pub code: i64,
    pub msg: String,
}

/// Binance server time response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTimeResponse {
    pub server_time: u64,
}

/// Binance account info response (GET /api/v3/account).
#[derive(Debug, Deserialize)]
pub struct AccountInfo {
    pub balances: Vec<AccountBalance>,
}

#[derive(Debug, Deserialize)]
pub struct AccountBalance {
    pub asset: String,
    #[serde(deserialize_with = "string_to_f64")]
    pub free: f64,
    #[serde(deserialize_with = "string_to_f64")]
    pub locked: f64,
}

/// Binance futures order response (POST /fapi/v1/order, RESULT/FULL-like fields).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesOrderResponse {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub price: f64,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub orig_qty: f64,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub executed_qty: f64,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub avg_price: f64,
    pub status: String,
    pub r#type: String,
    pub side: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesAllOrder {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    #[serde(deserialize_with = "string_or_number_to_f64_default")]
    pub price: f64,
    #[serde(deserialize_with = "string_or_number_to_f64_default")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "string_or_number_to_f64_default")]
    pub executed_qty: f64,
    #[serde(default, deserialize_with = "string_or_number_to_f64_default")]
    pub cum_quote: f64,
    #[serde(default, deserialize_with = "string_or_number_to_f64_default")]
    pub avg_price: f64,
    pub status: String,
    pub r#type: String,
    pub side: String,
    pub time: u64,
    pub update_time: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesUserTrade {
    pub symbol: String,
    pub id: u64,
    pub order_id: u64,
    #[serde(deserialize_with = "string_or_number_to_f64_default")]
    pub price: f64,
    #[serde(deserialize_with = "string_or_number_to_f64_default")]
    pub qty: f64,
    #[serde(default, deserialize_with = "string_or_number_to_f64_default")]
    pub commission: f64,
    #[serde(default)]
    pub commission_asset: String,
    pub time: u64,
    #[serde(default)]
    pub buyer: bool,
    #[serde(default)]
    pub maker: bool,
    #[serde(default, deserialize_with = "string_or_number_to_f64_default")]
    pub realized_pnl: f64,
}

/// Binance futures account info (GET /fapi/v2/account).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesAccountInfo {
    #[serde(default)]
    pub assets: Vec<BinanceFuturesAssetBalance>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceFuturesAssetBalance {
    pub asset: String,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub wallet_balance: f64,
    #[serde(default, deserialize_with = "string_to_f64")]
    pub available_balance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_trade_event() {
        let json = r#"{
            "e": "trade",
            "E": 1672515782136,
            "s": "BTCUSDT",
            "t": 12345,
            "p": "42000.50",
            "q": "0.001",
            "T": 1672515782136,
            "m": false
        }"#;
        let event: BinanceTradeEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.symbol, "BTCUSDT");
        assert!((event.price - 42000.50).abs() < f64::EPSILON);
        assert!((event.qty - 0.001).abs() < f64::EPSILON);
        assert_eq!(event.trade_id, 12345);
        assert!(!event.is_buyer_maker);
    }

    #[test]
    fn deserialize_order_response() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "orderId": 12345,
            "clientOrderId": "sq-test",
            "price": "0.00000000",
            "origQty": "0.00100000",
            "executedQty": "0.00100000",
            "status": "FILLED",
            "type": "MARKET",
            "side": "BUY",
            "fills": [
                {
                    "price": "42000.50000000",
                    "qty": "0.00100000",
                    "commission": "0.00000100",
                    "commissionAsset": "BTC"
                }
            ]
        }"#;
        let resp: BinanceOrderResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "FILLED");
        assert_eq!(resp.fills.len(), 1);
        assert!((resp.fills[0].price - 42000.50).abs() < 0.01);
    }

    #[test]
    fn deserialize_all_order_item() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "orderId": 28,
            "clientOrderId": "sq-abc12345",
            "price": "0.00000000",
            "origQty": "0.00100000",
            "executedQty": "0.00100000",
            "cummulativeQuoteQty": "42.50000000",
            "status": "FILLED",
            "timeInForce": "GTC",
            "type": "MARKET",
            "side": "BUY",
            "time": 1700000000000,
            "updateTime": 1700000001000,
            "isWorking": true,
            "workingTime": 1700000001000,
            "origQuoteOrderQty": "0.00000000",
            "selfTradePreventionMode": "NONE"
        }"#;
        let order: BinanceAllOrder = serde_json::from_str(json).unwrap();
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.order_id, 28);
        assert_eq!(order.status, "FILLED");
        assert!((order.executed_qty - 0.001).abs() < f64::EPSILON);
    }

    #[test]
    fn deserialize_my_trade_item() {
        let json = r#"{
            "symbol": "BTCUSDT",
            "id": 28457,
            "orderId": 100234,
            "price": "42000.50000000",
            "qty": "0.00100000",
            "commission": "0.00000100",
            "commissionAsset": "BTC",
            "time": 1700000001000,
            "isBuyer": true,
            "isMaker": false,
            "isBestMatch": true
        }"#;
        let trade: BinanceMyTrade = serde_json::from_str(json).unwrap();
        assert_eq!(trade.symbol, "BTCUSDT");
        assert_eq!(trade.order_id, 100234);
        assert!(trade.is_buyer);
        assert!(!trade.is_maker);
        assert!((trade.price - 42000.50).abs() < 0.01);
    }
}
