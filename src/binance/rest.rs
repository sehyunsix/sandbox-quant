use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use crate::error::AppError;
use crate::model::candle::Candle;
use crate::model::order::OrderSide;

use super::types::{
    AccountInfo, BinanceAllOrder, BinanceFuturesAccountInfo, BinanceFuturesAllOrder,
    BinanceFuturesOrderResponse, BinanceFuturesUserTrade, BinanceMyTrade, BinanceOrderResponse,
    ServerTimeResponse,
};

#[derive(Debug, Clone, Copy)]
pub struct SymbolOrderRules {
    pub min_qty: f64,
    pub max_qty: f64,
    pub step_size: f64,
    pub min_notional: Option<f64>,
}

pub struct BinanceRestClient {
    http: reqwest::Client,
    base_url: String,
    futures_base_url: String,
    api_key: String,
    secret_key: String,
    futures_api_key: String,
    futures_secret_key: String,
    recv_window: u64,
    time_offset_ms: AtomicI64,
    // Simple rate limiter: request count in current minute window
    request_count: AtomicU64,
    window_start: std::sync::Mutex<Instant>,
}

impl BinanceRestClient {
    pub fn new(
        base_url: &str,
        futures_base_url: &str,
        api_key: &str,
        secret_key: &str,
        futures_api_key: &str,
        futures_secret_key: &str,
        recv_window: u64,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.to_string(),
            futures_base_url: futures_base_url.to_string(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            futures_api_key: futures_api_key.to_string(),
            futures_secret_key: futures_secret_key.to_string(),
            recv_window,
            time_offset_ms: AtomicI64::new(0),
            request_count: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(Instant::now()),
        }
    }

    fn sign_with_secret(&self, query: &str, secret_key: &str) -> String {
        let offset = self.time_offset_ms.load(Ordering::Relaxed);
        let timestamp = chrono::Utc::now().timestamp_millis() + offset;
        let full_query = if query.is_empty() {
            format!("recvWindow={}&timestamp={}", self.recv_window, timestamp)
        } else {
            format!(
                "{}&recvWindow={}&timestamp={}",
                query, self.recv_window, timestamp
            )
        };
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret_key.as_bytes()).expect("HMAC key error");
        mac.update(full_query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        format!("{}&signature={}", full_query, signature)
    }

    fn sign(&self, query: &str) -> String {
        self.sign_with_secret(query, &self.secret_key)
    }

    fn sign_futures(&self, query: &str) -> String {
        self.sign_with_secret(query, &self.futures_secret_key)
    }

    async fn sync_time_offset(&self) -> Result<()> {
        let server_ms = self.server_time().await? as i64;
        let local_ms = chrono::Utc::now().timestamp_millis();
        let offset = server_ms - local_ms;
        self.time_offset_ms.store(offset, Ordering::Relaxed);
        tracing::warn!(
            offset_ms = offset,
            "Synchronized Binance server time offset"
        );
        Ok(())
    }

    fn parse_binance_api_error(body: &str) -> Option<super::types::BinanceApiErrorResponse> {
        serde_json::from_str::<super::types::BinanceApiErrorResponse>(body).ok()
    }

    fn check_rate_limit(&self) {
        let mut start = match self.window_start.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!("rate-limit mutex poisoned; continuing with recovered state");
                poisoned.into_inner()
            }
        };
        if start.elapsed().as_secs() >= 60 {
            *start = Instant::now();
            self.request_count.store(0, Ordering::Relaxed);
        }
        let count = self.request_count.fetch_add(1, Ordering::Relaxed);
        if count > 960 {
            tracing::warn!(count, "Approaching rate limit (80% of 1200/min)");
        }
    }

    pub async fn ping(&self) -> Result<()> {
        let url = format!("{}/api/v3/ping", self.base_url);
        self.http
            .get(&url)
            .send()
            .await
            .context("ping failed")?
            .error_for_status()
            .context("ping returned error status")?;
        Ok(())
    }

    pub async fn server_time(&self) -> Result<u64> {
        let url = format!("{}/api/v3/time", self.base_url);
        let resp: ServerTimeResponse = self
            .http
            .get(&url)
            .send()
            .await
            .context("server_time failed")?
            .json()
            .await?;
        Ok(resp.server_time)
    }

    pub async fn get_account(&self) -> Result<AccountInfo> {
        self.check_rate_limit();

        let signed = self.sign("");
        let url = format!("{}/api/v3/account?{}", self.base_url, signed);

        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .context("get_account HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Account request failed: {}", body));
        }

        Ok(resp.json().await?)
    }

    pub async fn get_futures_account(&self) -> Result<BinanceFuturesAccountInfo> {
        self.check_rate_limit();

        let signed = self.sign_futures("");
        let url = format!("{}/fapi/v2/account?{}", self.futures_base_url, signed);

        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.futures_api_key)
            .send()
            .await
            .context("get_futures_account HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Futures account request failed: {}", body));
        }

        Ok(resp.json().await?)
    }

    pub async fn place_market_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        client_order_id: &str,
    ) -> Result<BinanceOrderResponse> {
        self.check_rate_limit();

        let query = format!(
            "symbol={}&side={}&type=MARKET&quantity={:.5}&newClientOrderId={}&newOrderRespType=FULL",
            symbol,
            side.as_binance_str(),
            quantity,
            client_order_id,
        );
        let signed = self.sign(&query);
        let url = format!("{}/api/v3/order?{}", self.base_url, signed);

        tracing::info!(
            symbol,
            side = %side,
            quantity,
            client_order_id,
            "Placing market order"
        );

        let resp = self
            .http
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .context("place_market_order HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Order request failed: {}", body));
        }

        let order: BinanceOrderResponse = resp.json().await?;
        tracing::info!(
            order_id = order.order_id,
            status = %order.status,
            client_order_id = %order.client_order_id,
            "Order response received"
        );
        Ok(order)
    }

    pub async fn place_futures_market_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        client_order_id: &str,
    ) -> Result<BinanceOrderResponse> {
        self.check_rate_limit();

        let query = format!(
            "symbol={}&side={}&type=MARKET&quantity={:.5}&newClientOrderId={}&newOrderRespType=RESULT",
            symbol,
            side.as_binance_str(),
            quantity,
            client_order_id,
        );
        let signed = self.sign_futures(&query);
        let url = format!("{}/fapi/v1/order?{}", self.futures_base_url, signed);

        tracing::info!(
            symbol,
            side = %side,
            quantity,
            client_order_id,
            "Placing futures market order"
        );

        let resp = self
            .http
            .post(&url)
            .header("X-MBX-APIKEY", &self.futures_api_key)
            .send()
            .await
            .context("place_futures_market_order HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Futures order request failed: {}", body));
        }

        let fut: BinanceFuturesOrderResponse = resp.json().await?;
        let avg = if fut.avg_price > 0.0 {
            fut.avg_price
        } else if fut.price > 0.0 {
            fut.price
        } else {
            0.0
        };
        let fills = if fut.executed_qty > 0.0 && avg > 0.0 {
            vec![super::types::BinanceFill {
                price: avg,
                qty: fut.executed_qty,
                commission: 0.0,
                commission_asset: "USDT".to_string(),
            }]
        } else {
            Vec::new()
        };

        Ok(BinanceOrderResponse {
            symbol: fut.symbol,
            order_id: fut.order_id,
            client_order_id: fut.client_order_id,
            price: if fut.price > 0.0 { fut.price } else { avg },
            orig_qty: fut.orig_qty,
            executed_qty: fut.executed_qty,
            status: fut.status,
            r#type: fut.r#type,
            side: fut.side,
            fills,
        })
    }

    pub async fn place_futures_stop_market_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        stop_price: f64,
        client_order_id: &str,
    ) -> Result<BinanceOrderResponse> {
        self.check_rate_limit();

        let query = format!(
            "symbol={}&side={}&type=STOP_MARKET&quantity={:.5}&stopPrice={:.5}&reduceOnly=true&newClientOrderId={}&newOrderRespType=RESULT",
            symbol,
            side.as_binance_str(),
            quantity,
            stop_price,
            client_order_id,
        );
        let signed = self.sign_futures(&query);
        let url = format!("{}/fapi/v1/order?{}", self.futures_base_url, signed);

        tracing::info!(
            symbol,
            side = %side,
            quantity,
            stop_price,
            client_order_id,
            "Placing futures stop-market order"
        );

        let resp = self
            .http
            .post(&url)
            .header("X-MBX-APIKEY", &self.futures_api_key)
            .send()
            .await
            .context("place_futures_stop_market_order HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!(
                "Futures stop order request failed: {}",
                body
            ));
        }

        let fut: BinanceFuturesOrderResponse = resp.json().await?;
        let avg = if fut.avg_price > 0.0 {
            fut.avg_price
        } else if fut.price > 0.0 {
            fut.price
        } else {
            0.0
        };
        let fills = if fut.executed_qty > 0.0 && avg > 0.0 {
            vec![super::types::BinanceFill {
                price: avg,
                qty: fut.executed_qty,
                commission: 0.0,
                commission_asset: "USDT".to_string(),
            }]
        } else {
            Vec::new()
        };

        Ok(BinanceOrderResponse {
            symbol: fut.symbol,
            order_id: fut.order_id,
            client_order_id: fut.client_order_id,
            price: if fut.price > 0.0 { fut.price } else { avg },
            orig_qty: fut.orig_qty,
            executed_qty: fut.executed_qty,
            status: fut.status,
            r#type: fut.r#type,
            side: fut.side,
            fills,
        })
    }

    pub async fn get_spot_symbol_order_rules(&self, symbol: &str) -> Result<SymbolOrderRules> {
        let url = format!("{}/api/v3/exchangeInfo?symbol={}", self.base_url, symbol);
        let payload: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("get_spot_symbol_order_rules HTTP failed")?
            .error_for_status()
            .context("get_spot_symbol_order_rules returned error status")?
            .json()
            .await
            .context("get_spot_symbol_order_rules JSON parse failed")?;
        parse_symbol_order_rules_from_exchange_info(&payload, symbol, true)
    }

    pub async fn get_futures_symbol_order_rules(&self, symbol: &str) -> Result<SymbolOrderRules> {
        let url = format!(
            "{}/fapi/v1/exchangeInfo?symbol={}",
            self.futures_base_url, symbol
        );
        let payload: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("get_futures_symbol_order_rules HTTP failed")?
            .error_for_status()
            .context("get_futures_symbol_order_rules returned error status")?
            .json()
            .await
            .context("get_futures_symbol_order_rules JSON parse failed")?;
        parse_symbol_order_rules_from_exchange_info(&payload, symbol, false)
    }

    /// Fetch historical kline (candlestick) OHLC data.
    /// Returns `Vec<Candle>` oldest first.
    pub async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        self.get_klines_for_market(symbol, interval, limit, false)
            .await
    }

    pub async fn get_klines_for_market(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
        is_futures: bool,
    ) -> Result<Vec<Candle>> {
        self.check_rate_limit();

        let url = if is_futures {
            format!(
                "{}/fapi/v1/klines?symbol={}&interval={}&limit={}",
                self.futures_base_url, symbol, interval, limit,
            )
        } else {
            format!(
                "{}/api/v3/klines?symbol={}&interval={}&limit={}",
                self.base_url, symbol, interval, limit,
            )
        };

        let resp: Vec<Vec<serde_json::Value>> = self
            .http
            .get(&url)
            .send()
            .await
            .context("get_klines HTTP failed")?
            .error_for_status()
            .context("get_klines returned error status")?
            .json()
            .await
            .context("get_klines JSON parse failed")?;

        let candles: Vec<Candle> = resp
            .iter()
            .filter_map(|kline| {
                let open_time = kline.get(0)?.as_u64()?;
                let open = kline.get(1)?.as_str()?.parse::<f64>().ok()?;
                let high = kline.get(2)?.as_str()?.parse::<f64>().ok()?;
                let low = kline.get(3)?.as_str()?.parse::<f64>().ok()?;
                let close = kline.get(4)?.as_str()?.parse::<f64>().ok()?;
                // Binance kline close time is inclusive end ms; convert to half-open [open, close+1).
                let close_time = kline
                    .get(6)?
                    .as_u64()
                    .map(|v| v.saturating_add(1))
                    .unwrap_or(open_time.saturating_add(60_000));
                Some(Candle {
                    open,
                    high,
                    low,
                    close,
                    open_time,
                    close_time,
                })
            })
            .collect();

        Ok(candles)
    }

    pub async fn cancel_order(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<BinanceOrderResponse> {
        self.check_rate_limit();

        let query = format!("symbol={}&origClientOrderId={}", symbol, client_order_id);
        let signed = self.sign(&query);
        let url = format!("{}/api/v3/order?{}", self.base_url, signed);

        tracing::info!(symbol, client_order_id, "Cancelling order");

        let resp = self
            .http
            .delete(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .context("cancel_order HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Cancel request failed: {}", body));
        }

        Ok(resp.json().await?)
    }

    /// Fetch one page of orders for a symbol.
    async fn get_all_orders_page(
        &self,
        symbol: &str,
        limit: usize,
        from_order_id: Option<u64>,
    ) -> Result<Vec<BinanceAllOrder>> {
        self.check_rate_limit();

        let limit = limit.clamp(1, 1000);
        let query = match from_order_id {
            Some(order_id) => format!("symbol={}&limit={}&orderId={}", symbol, limit, order_id),
            None => format!("symbol={}&limit={}", symbol, limit),
        };
        for attempt in 0..=1 {
            let signed = self.sign(&query);
            let url = format!("{}/api/v3/allOrders?{}", self.base_url, signed);

            let resp = self
                .http
                .get(&url)
                .header("X-MBX-APIKEY", &self.api_key)
                .send()
                .await
                .context("get_all_orders HTTP failed")?;

            if resp.status().is_success() {
                return Ok(resp.json().await?);
            }

            let body = resp.text().await.unwrap_or_default();
            if let Some(err) = Self::parse_binance_api_error(&body) {
                if err.code == -1021 && attempt == 0 {
                    tracing::warn!("allOrders got -1021; syncing server time and retrying once");
                    self.sync_time_offset().await?;
                    continue;
                }
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("All orders request failed: {}", body));
        }

        Err(anyhow::anyhow!("All orders request failed after retry"))
    }

    /// Fetch recent orders for a symbol from `/api/v3/allOrders`.
    /// `limit` controls max rows returned (1..=1000).
    pub async fn get_all_orders(&self, symbol: &str, limit: usize) -> Result<Vec<BinanceAllOrder>> {
        self.get_all_orders_page(symbol, limit, None).await
    }

    async fn get_futures_all_orders_page(
        &self,
        symbol: &str,
        limit: usize,
        from_order_id: Option<u64>,
    ) -> Result<Vec<BinanceAllOrder>> {
        self.check_rate_limit();
        let limit = limit.clamp(1, 1000);
        let query = match from_order_id {
            Some(order_id) => format!("symbol={}&limit={}&orderId={}", symbol, limit, order_id),
            None => format!("symbol={}&limit={}", symbol, limit),
        };
        let signed = self.sign_futures(&query);
        let url = format!("{}/fapi/v1/allOrders?{}", self.futures_base_url, signed);
        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.futures_api_key)
            .send()
            .await
            .context("get_futures_all_orders HTTP failed")?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Some(err) = Self::parse_binance_api_error(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!(
                "Futures allOrders request failed: {}",
                body
            ));
        }
        let rows: Vec<BinanceFuturesAllOrder> = resp.json().await?;
        Ok(rows
            .into_iter()
            .map(|o| {
                let cumm_quote = if o.cum_quote > 0.0 {
                    o.cum_quote
                } else {
                    o.avg_price * o.executed_qty
                };
                BinanceAllOrder {
                    symbol: o.symbol,
                    order_id: o.order_id,
                    client_order_id: o.client_order_id,
                    price: o.price,
                    orig_qty: o.orig_qty,
                    executed_qty: o.executed_qty,
                    cummulative_quote_qty: cumm_quote,
                    status: o.status,
                    r#type: o.r#type,
                    side: o.side,
                    time: o.time,
                    update_time: o.update_time,
                }
            })
            .collect())
    }

    pub async fn get_futures_all_orders(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<Vec<BinanceAllOrder>> {
        self.get_futures_all_orders_page(symbol, limit, None).await
    }

    async fn get_my_trades_page(
        &self,
        symbol: &str,
        limit: usize,
        from_id: Option<u64>,
    ) -> Result<Vec<BinanceMyTrade>> {
        self.check_rate_limit();

        let limit = limit.clamp(1, 1000);
        let query = match from_id {
            Some(v) => format!("symbol={}&limit={}&fromId={}", symbol, limit, v),
            None => format!("symbol={}&limit={}", symbol, limit),
        };
        for attempt in 0..=1 {
            let signed = self.sign(&query);
            let url = format!("{}/api/v3/myTrades?{}", self.base_url, signed);

            let resp = self
                .http
                .get(&url)
                .header("X-MBX-APIKEY", &self.api_key)
                .send()
                .await
                .context("get_my_trades HTTP failed")?;

            if resp.status().is_success() {
                return Ok(resp.json().await?);
            }

            let body = resp.text().await.unwrap_or_default();
            if let Some(err) = Self::parse_binance_api_error(&body) {
                if err.code == -1021 && attempt == 0 {
                    tracing::warn!("myTrades got -1021; syncing server time and retrying once");
                    self.sync_time_offset().await?;
                    continue;
                }
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("My trades request failed: {}", body));
        }

        Err(anyhow::anyhow!("My trades request failed after retry"))
    }

    /// Fetch recent personal trades for a symbol.
    pub async fn get_my_trades(&self, symbol: &str, limit: usize) -> Result<Vec<BinanceMyTrade>> {
        self.get_my_trades_page(symbol, limit, None).await
    }

    /// Fetch all personal trades from the oldest side (fromId=0), up to `max_total`.
    pub async fn get_my_trades_history(
        &self,
        symbol: &str,
        max_total: usize,
    ) -> Result<Vec<BinanceMyTrade>> {
        let page_size = 1000usize;
        let target = max_total.max(1);
        let mut out = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let page = self
                .get_my_trades_page(
                    symbol,
                    page_size.min(target.saturating_sub(out.len())),
                    Some(cursor),
                )
                .await?;
            if page.is_empty() {
                break;
            }
            let fetched = page.len();
            let mut max_trade_id = cursor;
            for t in page {
                max_trade_id = max_trade_id.max(t.id);
                out.push(t);
                if out.len() >= target {
                    break;
                }
            }
            if out.len() >= target || fetched < page_size {
                break;
            }
            let next = max_trade_id.saturating_add(1);
            if next <= cursor {
                break;
            }
            cursor = next;
        }

        Ok(out)
    }

    /// Fetch new personal trades since `from_id` (inclusive), paging forward.
    pub async fn get_my_trades_since(
        &self,
        symbol: &str,
        from_id: u64,
        max_pages: usize,
    ) -> Result<Vec<BinanceMyTrade>> {
        let page_size = 1000usize;
        let mut out = Vec::new();
        let mut cursor = from_id;
        let mut pages = 0usize;

        while pages < max_pages.max(1) {
            let page = self
                .get_my_trades_page(symbol, page_size, Some(cursor))
                .await?;
            if page.is_empty() {
                break;
            }
            pages += 1;
            let fetched = page.len();
            let mut max_trade_id = cursor;
            for t in page {
                max_trade_id = max_trade_id.max(t.id);
                out.push(t);
            }
            if fetched < page_size {
                break;
            }
            let next = max_trade_id.saturating_add(1);
            if next <= cursor {
                break;
            }
            cursor = next;
        }

        Ok(out)
    }

    async fn get_futures_my_trades_page(
        &self,
        symbol: &str,
        limit: usize,
        from_id: Option<u64>,
    ) -> Result<Vec<BinanceMyTrade>> {
        self.check_rate_limit();
        let limit = limit.clamp(1, 1000);
        let query = match from_id {
            Some(v) => format!("symbol={}&limit={}&fromId={}", symbol, limit, v),
            None => format!("symbol={}&limit={}", symbol, limit),
        };
        let signed = self.sign_futures(&query);
        let url = format!("{}/fapi/v1/userTrades?{}", self.futures_base_url, signed);
        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.futures_api_key)
            .send()
            .await
            .context("get_futures_my_trades HTTP failed")?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Some(err) = Self::parse_binance_api_error(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("Futures myTrades request failed: {}", body));
        }
        let rows: Vec<BinanceFuturesUserTrade> = resp.json().await?;
        Ok(rows
            .into_iter()
            .map(|t| BinanceMyTrade {
                symbol: t.symbol,
                id: t.id,
                order_id: t.order_id,
                price: t.price,
                qty: t.qty,
                commission: t.commission,
                commission_asset: t.commission_asset,
                time: t.time,
                is_buyer: t.buyer,
                is_maker: t.maker,
                realized_pnl: t.realized_pnl,
            })
            .collect())
    }

    pub async fn get_futures_my_trades_history(
        &self,
        symbol: &str,
        max_total: usize,
    ) -> Result<Vec<BinanceMyTrade>> {
        let page_size = 1000usize;
        let target = max_total.max(1);
        let mut out = Vec::new();
        let mut cursor: u64 = 0;
        loop {
            let page = self
                .get_futures_my_trades_page(
                    symbol,
                    page_size.min(target.saturating_sub(out.len())),
                    Some(cursor),
                )
                .await?;
            if page.is_empty() {
                break;
            }
            let fetched = page.len();
            let mut max_trade_id = cursor;
            for t in page {
                max_trade_id = max_trade_id.max(t.id);
                out.push(t);
                if out.len() >= target {
                    break;
                }
            }
            if out.len() >= target || fetched < page_size {
                break;
            }
            let next = max_trade_id.saturating_add(1);
            if next <= cursor {
                break;
            }
            cursor = next;
        }
        Ok(out)
    }
}

fn parse_symbol_order_rules_from_exchange_info(
    payload: &serde_json::Value,
    symbol: &str,
    prefer_market_lot_size: bool,
) -> Result<SymbolOrderRules> {
    let symbols = payload
        .get("symbols")
        .and_then(|v| v.as_array())
        .context("exchangeInfo missing symbols")?;
    let symbol_row = symbols
        .iter()
        .find(|row| row.get("symbol").and_then(|v| v.as_str()) == Some(symbol))
        .with_context(|| format!("exchangeInfo symbol not found: {}", symbol))?;
    let filters = symbol_row
        .get("filters")
        .and_then(|v| v.as_array())
        .context("exchangeInfo symbol missing filters")?;

    let primary_type = if prefer_market_lot_size {
        "MARKET_LOT_SIZE"
    } else {
        "LOT_SIZE"
    };
    let fallback_type = if prefer_market_lot_size {
        "LOT_SIZE"
    } else {
        "MARKET_LOT_SIZE"
    };
    let parsed = find_filter(filters, primary_type)
        .and_then(parse_lot_filter_values)
        .or_else(|| find_filter(filters, fallback_type).and_then(parse_lot_filter_values))
        .context("exchangeInfo missing valid LOT_SIZE/MARKET_LOT_SIZE")?;
    let (min_qty, max_qty, step_size) = parsed;

    let min_notional = find_filter(filters, "MIN_NOTIONAL")
        .and_then(|f| f.get("notional").or_else(|| f.get("minNotional")))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    Ok(SymbolOrderRules {
        min_qty,
        max_qty,
        step_size,
        min_notional,
    })
}

fn find_filter<'a>(
    filters: &'a [serde_json::Value],
    filter_type: &str,
) -> Option<&'a serde_json::Value> {
    filters
        .iter()
        .find(|f| f.get("filterType").and_then(|v| v.as_str()) == Some(filter_type))
}

fn parse_lot_filter_values(filter: &serde_json::Value) -> Option<(f64, f64, f64)> {
    let min_qty = json_str_to_f64(filter, "minQty").ok()?;
    let max_qty = json_str_to_f64(filter, "maxQty").ok()?;
    let step_size = json_str_to_f64(filter, "stepSize").ok()?;
    if step_size <= 0.0 {
        return None;
    }
    Some((min_qty, max_qty, step_size))
}

fn json_str_to_f64(row: &serde_json::Value, key: &str) -> Result<f64> {
    let s = row
        .get(key)
        .and_then(|v| v.as_str())
        .with_context(|| format!("missing field {}", key))?;
    s.parse::<f64>()
        .with_context(|| format!("invalid {} value {}", key, s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hmac_signing_produces_hex_signature() {
        let client = BinanceRestClient::new(
            "https://testnet.binance.vision",
            "https://testnet.binancefuture.com",
            "test_key",
            "test_secret",
            "test_fut_key",
            "test_fut_secret",
            5000,
        );
        let signed = client.sign("symbol=BTCUSDT&side=BUY");
        // Should contain original query, recvWindow, timestamp, and signature
        assert!(signed.contains("symbol=BTCUSDT&side=BUY"));
        assert!(signed.contains("recvWindow=5000"));
        assert!(signed.contains("timestamp="));
        assert!(signed.contains("&signature="));

        // Signature should be 64-char hex (SHA256)
        let sig = signed.split("&signature=").nth(1).unwrap();
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hmac_known_vector() {
        // Binance docs example: queryString with known secret should produce known signature
        let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";
        let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        assert_eq!(
            signature,
            "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71"
        );
    }

    #[test]
    fn check_rate_limit_does_not_panic_on_poisoned_mutex() {
        let client = BinanceRestClient::new(
            "https://testnet.binance.vision",
            "https://testnet.binancefuture.com",
            "test_key",
            "test_secret",
            "test_fut_key",
            "test_fut_secret",
            5000,
        );

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = client.window_start.lock().unwrap();
            panic!("poison window_start mutex");
        }));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.check_rate_limit();
        }));
        assert!(
            result.is_ok(),
            "check_rate_limit should recover from poison"
        );
    }

    #[test]
    fn parse_symbol_rules_prefers_market_lot_size_for_spot() {
        let payload = json!({
            "symbols": [{
                "symbol": "BTCUSDT",
                "filters": [
                    {"filterType":"LOT_SIZE","minQty":"0.00100000","maxQty":"100.00000000","stepSize":"0.00100000"},
                    {"filterType":"MARKET_LOT_SIZE","minQty":"0.00001000","maxQty":"50.00000000","stepSize":"0.00001000"},
                    {"filterType":"MIN_NOTIONAL","minNotional":"5.00000000"}
                ]
            }]
        });
        let rules = parse_symbol_order_rules_from_exchange_info(&payload, "BTCUSDT", true).unwrap();
        assert!((rules.step_size - 0.00001).abs() < 1e-12);
        assert!((rules.min_qty - 0.00001).abs() < 1e-12);
        assert_eq!(rules.min_notional, Some(5.0));
    }

    #[test]
    fn parse_symbol_rules_uses_lot_size_for_futures() {
        let payload = json!({
            "symbols": [{
                "symbol": "ETHUSDT",
                "filters": [
                    {"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"10000","stepSize":"0.001"},
                    {"filterType":"MARKET_LOT_SIZE","minQty":"0.01","maxQty":"1000","stepSize":"0.01"}
                ]
            }]
        });
        let rules =
            parse_symbol_order_rules_from_exchange_info(&payload, "ETHUSDT", false).unwrap();
        assert!((rules.step_size - 0.001).abs() < 1e-12);
        assert!((rules.min_qty - 0.001).abs() < 1e-12);
    }

    #[test]
    fn parse_symbol_rules_fallback_when_market_lot_size_is_invalid() {
        let payload = json!({
            "symbols": [{
                "symbol": "BTCUSDT",
                "filters": [
                    {"filterType":"LOT_SIZE","minQty":"0.00001000","maxQty":"50.00000000","stepSize":"0.00001000"},
                    {"filterType":"MARKET_LOT_SIZE","minQty":"0.00001000","maxQty":"50.00000000","stepSize":"0.00000000"}
                ]
            }]
        });
        let rules = parse_symbol_order_rules_from_exchange_info(&payload, "BTCUSDT", true).unwrap();
        assert!((rules.step_size - 0.00001).abs() < 1e-12);
    }
}
