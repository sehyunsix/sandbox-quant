use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::error::AppError;
use crate::model::order::OrderSide;

use super::types::{BinanceOrderResponse, ServerTimeResponse};

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
        let full_query = format!(
            "{}&recvWindow={}&timestamp={}",
            query, self.recv_window, timestamp
        );
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

    pub async fn cancel_order(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<BinanceOrderResponse> {
        self.check_rate_limit();

        let query = format!(
            "symbol={}&origClientOrderId={}",
            symbol, client_order_id
        );
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
