use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::error::AppError;
use crate::model::candle::Candle;
use crate::model::order::OrderSide;

use super::types::{
    AccountInfo, BinanceAllOrder, BinanceMyTrade, BinanceOrderResponse, ServerTimeResponse,
};

pub struct BinanceRestClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    secret_key: String,
    recv_window: u64,
    // Simple rate limiter: request count in current minute window
    request_count: AtomicU64,
    window_start: std::sync::Mutex<Instant>,
}

impl BinanceRestClient {
    pub fn new(base_url: &str, api_key: &str, secret_key: &str, recv_window: u64) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            recv_window,
            request_count: AtomicU64::new(0),
            window_start: std::sync::Mutex::new(Instant::now()),
        }
    }

    fn sign(&self, query: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp_millis();
        let full_query = if query.is_empty() {
            format!("recvWindow={}&timestamp={}", self.recv_window, timestamp)
        } else {
            format!(
                "{}&recvWindow={}&timestamp={}",
                query, self.recv_window, timestamp
            )
        };
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.secret_key.as_bytes()).expect("HMAC key error");
        mac.update(full_query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        format!("{}&signature={}", full_query, signature)
    }

    fn check_rate_limit(&self) {
        let mut start = self.window_start.lock().unwrap();
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

    /// Fetch historical kline (candlestick) OHLC data.
    /// Returns `Vec<Candle>` oldest first.
    pub async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        self.check_rate_limit();

        let url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&limit={}",
            self.base_url, symbol, interval, limit,
        );

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
                let volume = kline.get(5)?.as_str()?.parse::<f64>().ok()?;
                let close_time = kline.get(6)?.as_u64()?;
                Some(Candle {
                    open,
                    high,
                    low,
                    close,
                    volume,
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
        let signed = self.sign(&query);
        let url = format!("{}/api/v3/allOrders?{}", self.base_url, signed);

        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .context("get_all_orders HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("All orders request failed: {}", body));
        }

        Ok(resp.json().await?)
    }

    /// Fetch all available orders for a symbol by paging through `/api/v3/allOrders`.
    /// `page_size` controls each request size (1..=1000).
    pub async fn get_all_orders(
        &self,
        symbol: &str,
        page_size: usize,
    ) -> Result<Vec<BinanceAllOrder>> {
        let mut all_orders = Vec::new();
        let mut seen_order_ids = HashSet::new();
        // Start from the oldest cursor so we can reliably walk beyond 1000 orders.
        let mut from_order_id: Option<u64> = Some(0);
        let page_size = page_size.clamp(1, 1000);

        loop {
            let page = self
                .get_all_orders_page(symbol, page_size, from_order_id)
                .await?;

            if page.is_empty() {
                break;
            }

            let fetched = page.len();
            let mut max_seen_in_page = 0_u64;
            for order in page {
                max_seen_in_page = max_seen_in_page.max(order.order_id);
                if seen_order_ids.insert(order.order_id) {
                    all_orders.push(order);
                }
            }

            if fetched < page_size {
                break;
            }

            let current_cursor = from_order_id.unwrap_or(0);
            let next_cursor = max_seen_in_page.saturating_add(1);
            if next_cursor <= current_cursor {
                break;
            }
            from_order_id = Some(next_cursor);
        }

        Ok(all_orders)
    }

    /// Fetch one page of account trades for a symbol.
    async fn get_my_trades_page(
        &self,
        symbol: &str,
        limit: usize,
        from_id: Option<u64>,
    ) -> Result<Vec<BinanceMyTrade>> {
        self.check_rate_limit();

        let limit = limit.clamp(1, 1000);
        let query = match from_id {
            Some(id) => format!("symbol={}&limit={}&fromId={}", symbol, limit, id),
            None => format!("symbol={}&limit={}", symbol, limit),
        };
        let signed = self.sign(&query);
        let url = format!("{}/api/v3/myTrades?{}", self.base_url, signed);

        let resp = self
            .http
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await
            .context("get_my_trades HTTP failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<super::types::BinanceApiErrorResponse>(&body) {
                return Err(AppError::BinanceApi {
                    code: err.code,
                    msg: err.msg,
                }
                .into());
            }
            return Err(anyhow::anyhow!("My trades request failed: {}", body));
        }

        Ok(resp.json().await?)
    }

    /// Fetch all available account trades for a symbol by paging through `/api/v3/myTrades`.
    pub async fn get_my_trades(
        &self,
        symbol: &str,
        page_size: usize,
    ) -> Result<Vec<BinanceMyTrade>> {
        let mut all_trades = Vec::new();
        let mut seen_trade_ids = HashSet::new();
        let mut from_id: Option<u64> = Some(0);
        let page_size = page_size.clamp(1, 1000);

        loop {
            let page = self.get_my_trades_page(symbol, page_size, from_id).await?;

            if page.is_empty() {
                break;
            }

            let fetched = page.len();
            let mut max_seen_in_page = 0_u64;
            for trade in page {
                max_seen_in_page = max_seen_in_page.max(trade.id);
                if seen_trade_ids.insert(trade.id) {
                    all_trades.push(trade);
                }
            }

            if fetched < page_size {
                break;
            }

            let current_cursor = from_id.unwrap_or(0);
            let next_cursor = max_seen_in_page.saturating_add(1);
            if next_cursor <= current_cursor {
                break;
            }
            from_id = Some(next_cursor);
        }

        Ok(all_trades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_signing_produces_hex_signature() {
        let client = BinanceRestClient::new(
            "https://testnet.binance.vision",
            "test_key",
            "test_secret",
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
}
