use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite;
use tungstenite::error::{Error as WsError, ProtocolError, UrlError};
use tungstenite::protocol::frame::coding::CloseCode;

use super::types::{BinanceExecutionReport, BinanceTradeEvent};
use crate::event::{AppEvent, WsConnectionStatus};
use crate::model::tick::Tick;

/// Exponential backoff for reconnection.
struct ExponentialBackoff {
    current: Duration,
    initial: Duration,
    max: Duration,
    factor: f64,
}

impl ExponentialBackoff {
    fn new(initial: Duration, max: Duration, factor: f64) -> Self {
        Self {
            current: initial,
            initial,
            max,
            factor,
        }
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = Duration::from_secs_f64(
            (self.current.as_secs_f64() * self.factor).min(self.max.as_secs_f64()),
        );
        delay
    }

    fn reset(&mut self) {
        self.current = self.initial;
    }
}

pub struct BinanceWsClient {
    url: String,
}

impl BinanceWsClient {
    /// Create a new WebSocket client.
    ///
    /// `ws_base_url` — e.g. `wss://stream.testnet.binance.vision/ws`
    pub fn new(ws_base_url: &str) -> Self {
        Self {
            url: ws_base_url.to_string(),
        }
    }

    pub fn trade_stream(symbol: &str) -> String {
        format!("{}@trade", symbol.to_lowercase())
    }

    /// Connect and run the WebSocket loop with automatic reconnection.
    /// Sends WsStatus events through `status_tx` and ticks through `tick_tx`.
    pub async fn connect_and_run(
        &self,
        tick_tx: mpsc::Sender<Tick>,
        status_tx: mpsc::Sender<AppEvent>,
        mut symbol_rx: watch::Receiver<String>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut backoff =
            ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(60), 2.0);
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            let symbol = symbol_rx.borrow().clone();
            match self
                .connect_once(&tick_tx, &status_tx, &symbol, &mut symbol_rx, &mut shutdown)
                .await
            {
                Ok(()) => {
                    // Clean shutdown requested
                    let _ = status_tx
                        .send(AppEvent::WsStatus(WsConnectionStatus::Disconnected))
                        .await;
                    break;
                }
                Err(e) => {
                    if e.to_string() == "__WS_SYMBOL_CHANGED__" {
                        let _ = status_tx
                            .send(AppEvent::LogMessage(
                                "Switching market stream...".to_string(),
                            ))
                            .await;
                        continue;
                    }

                    let _ = status_tx
                        .send(AppEvent::WsStatus(WsConnectionStatus::Disconnected))
                        .await;
                    tracing::warn!(attempt, error = %e, "WS connection attempt failed");
                    let _ = status_tx
                        .send(AppEvent::LogMessage(format!(
                            "WS error (attempt #{}): {}",
                            attempt, e
                        )))
                        .await;

                    let delay = backoff.next_delay();
                    let _ = status_tx
                        .send(AppEvent::WsStatus(WsConnectionStatus::Reconnecting {
                            attempt,
                            delay_ms: delay.as_millis() as u64,
                        }))
                        .await;

                    tokio::select! {
                        _ = tokio::time::sleep(delay) => continue,
                        _ = shutdown.changed() => {
                            let _ = status_tx
                                .send(AppEvent::LogMessage("Shutdown during reconnect".to_string()))
                                .await;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn connect_once(
        &self,
        tick_tx: &mpsc::Sender<Tick>,
        status_tx: &mpsc::Sender<AppEvent>,
        symbol: &str,
        symbol_rx: &mut watch::Receiver<String>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let _ = status_tx
            .send(AppEvent::LogMessage(format!("Connecting to {}", self.url)))
            .await;

        let (ws_stream, resp) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| {
                let detail = format_ws_error(&e);
                let _ = status_tx.try_send(AppEvent::LogMessage(detail.clone()));
                anyhow::anyhow!("WebSocket connect failed: {}", detail)
            })?;

        tracing::debug!(status = %resp.status(), "WebSocket HTTP upgrade response");

        let (mut write, mut read) = ws_stream.split();

        // Send SUBSCRIBE message per Binance WebSocket API spec
        let streams = vec![Self::trade_stream(symbol)];
        let subscribe_msg = serde_json::json!({
            "method": "SUBSCRIBE",
            "params": streams,
            "id": 1
        });
        write
            .send(tungstenite::Message::Text(subscribe_msg.to_string()))
            .await
            .map_err(|e| {
                let detail = format_ws_error(&e);
                anyhow::anyhow!("Failed to send SUBSCRIBE: {}", detail)
            })?;

        let _ = status_tx
            .send(AppEvent::LogMessage(format!(
                "Subscribed to: {}",
                streams.join(", ")
            )))
            .await;

        // Send Connected AFTER successful subscription
        let _ = status_tx
            .send(AppEvent::WsStatus(WsConnectionStatus::Connected))
            .await;
        let _ = status_tx
            .send(AppEvent::LogMessage("WebSocket connected".to_string()))
            .await;

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(tungstenite::Message::Text(text))) => {
                            self.handle_text_message(&text, tick_tx).await;
                        }
                        Some(Ok(tungstenite::Message::Ping(_))) => {
                            // tokio-tungstenite handles pong automatically
                        }
                        Some(Ok(tungstenite::Message::Close(frame))) => {
                            let detail = match &frame {
                                Some(cf) => format!(
                                    "Server closed: code={} reason=\"{}\"",
                                    format_close_code(&cf.code),
                                    cf.reason
                                ),
                                None => "Server closed: no close frame".to_string(),
                            };
                            let _ = status_tx
                                .send(AppEvent::LogMessage(detail.clone()))
                                .await;
                            return Err(anyhow::anyhow!("{}", detail));
                        }
                        Some(Ok(other)) => {
                            tracing::trace!(msg_type = ?other, "Unhandled WS message type");
                        }
                        Some(Err(e)) => {
                            let detail = format_ws_error(&e);
                            let _ = status_tx
                                .send(AppEvent::LogMessage(format!("WS read error: {}", detail)))
                                .await;
                            return Err(anyhow::anyhow!("WebSocket read error: {}", detail));
                        }
                        None => {
                            return Err(anyhow::anyhow!(
                                "WebSocket stream ended unexpectedly (connection dropped)"
                            ));
                        }
                    }
                }
                _ = shutdown.changed() => {
                    // Send UNSUBSCRIBE before closing
                    let unsub_msg = serde_json::json!({
                        "method": "UNSUBSCRIBE",
                        "params": streams,
                        "id": 2
                    });
                    let _ = write
                        .send(tungstenite::Message::Text(unsub_msg.to_string()))
                        .await;
                    let _ = write.send(tungstenite::Message::Close(None)).await;
                    return Ok(());
                }
                _ = symbol_rx.changed() => {
                    let unsub_msg = serde_json::json!({
                        "method": "UNSUBSCRIBE",
                        "params": streams,
                        "id": 3
                    });
                    let _ = write
                        .send(tungstenite::Message::Text(unsub_msg.to_string()))
                        .await;
                    let _ = write.send(tungstenite::Message::Close(None)).await;
                    return Err(anyhow::anyhow!("__WS_SYMBOL_CHANGED__"));
                }
            }
        }
    }

    async fn handle_text_message(
        &self,
        text: &str,
        tick_tx: &mpsc::Sender<Tick>,
    ) {
        let value: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, raw = %text, "Failed to parse WS message to JSON Value");
                return;
            }
        };

        // Skip subscription confirmation responses like {"result":null,"id":1}
        if value.get("result").is_some() && value.get("id").is_some() {
            tracing::debug!(id = %value["id"], "Subscription response received");
            return;
        }

        if let Some(event_type) = value.get("e").and_then(|v| v.as_str()) {
            match event_type {
                "trade" => match serde_json::from_value::<BinanceTradeEvent>(value) {
                    Ok(event) => {
                        let tick = Tick {
                            price: event.price,
                            qty: event.qty,
                            timestamp_ms: event.event_time,
                            is_buyer_maker: event.is_buyer_maker,
                            trade_id: event.trade_id,
                        };
                        if tick_tx.try_send(tick).is_err() {
                            tracing::warn!("Tick channel full, dropping tick");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse BinanceTradeEvent");
                    }
                },
                "executionReport" => {
                    match serde_json::from_value::<BinanceExecutionReport>(value) {
                        Ok(report) => {
                            if report.order_status == "FILLED" {
                                tracing::info!(
                                    trace_id = %report.client_order_id,
                                    order_id = report.order_id,
                                    status = %report.order_status,
                                    "Execution report 'FILLED' received via WebSocket"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to parse BinanceExecutionReport");
                        }
                    }
                }
                _ => {
                    tracing::trace!(event_type, "Unhandled WS event type");
                }
            }
        } else {
            tracing::trace!(raw = %text, "WS message without event type 'e'");
        }
    }
}

/// Format a tungstenite WebSocket error into a detailed, human-readable string.
fn format_ws_error(err: &WsError) -> String {
    match err {
        WsError::ConnectionClosed => "Connection closed normally".to_string(),
        WsError::AlreadyClosed => "Attempted operation on already-closed connection".to_string(),
        WsError::Io(io_err) => {
            format!("IO error [kind={}]: {}", io_err.kind(), io_err)
        }
        WsError::Tls(tls_err) => format!("TLS error: {}", tls_err),
        WsError::Capacity(cap_err) => format!("Capacity error: {}", cap_err),
        WsError::Protocol(proto_err) => {
            let detail = match proto_err {
                ProtocolError::ResetWithoutClosingHandshake => {
                    "connection reset without closing handshake (server may have dropped)"
                }
                ProtocolError::SendAfterClosing => "tried to send after close frame",
                ProtocolError::ReceivedAfterClosing => "received data after close frame",
                ProtocolError::HandshakeIncomplete => "handshake incomplete",
                _ => "",
            };
            if detail.is_empty() {
                format!("Protocol error: {}", proto_err)
            } else {
                format!("Protocol error: {} ({})", proto_err, detail)
            }
        }
        WsError::WriteBufferFull(_) => "Write buffer full (backpressure)".to_string(),
        WsError::Utf8 => "UTF-8 encoding error in frame data".to_string(),
        WsError::AttackAttempt => "Attack attempt detected by WebSocket library".to_string(),
        WsError::Url(url_err) => {
            let hint = match url_err {
                UrlError::TlsFeatureNotEnabled => "TLS feature not compiled in",
                UrlError::NoHostName => "no host name in URL",
                UrlError::UnableToConnect(addr) => {
                    return format!(
                        "URL error: unable to connect to {} (DNS/network failure?)",
                        addr
                    );
                }
                UrlError::UnsupportedUrlScheme => "only ws:// or wss:// are supported",
                UrlError::EmptyHostName => "empty host name in URL",
                UrlError::NoPathOrQuery => "no path/query in URL",
            };
            format!("URL error: {} — {}", url_err, hint)
        }
        WsError::Http(resp) => {
            let status = resp.status();
            let body_preview = resp
                .body()
                .as_ref()
                .and_then(|b| std::str::from_utf8(b).ok())
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();
            format!(
                "HTTP error: status={} ({}), body=\"{}\"",
                status.as_u16(),
                status.canonical_reason().unwrap_or("unknown"),
                body_preview
            )
        }
        WsError::HttpFormat(e) => format!("HTTP format error: {}", e),
    }
}

/// Format a WebSocket close code into a readable string with numeric value.
fn format_close_code(code: &CloseCode) -> String {
    let (num, label) = match code {
        CloseCode::Normal => (1000, "Normal"),
        CloseCode::Away => (1001, "Going Away"),
        CloseCode::Protocol => (1002, "Protocol Error"),
        CloseCode::Unsupported => (1003, "Unsupported Data"),
        CloseCode::Status => (1005, "No Status"),
        CloseCode::Abnormal => (1006, "Abnormal Closure"),
        CloseCode::Invalid => (1007, "Invalid Payload"),
        CloseCode::Policy => (1008, "Policy Violation"),
        CloseCode::Size => (1009, "Message Too Big"),
        CloseCode::Extension => (1010, "Extension Required"),
        CloseCode::Error => (1011, "Internal Error"),
        CloseCode::Restart => (1012, "Service Restart"),
        CloseCode::Again => (1013, "Try Again Later"),
        CloseCode::Tls => (1015, "TLS Handshake Failure"),
        CloseCode::Reserved(n) => (*n, "Reserved"),
        CloseCode::Iana(n) => (*n, "IANA"),
        CloseCode::Library(n) => (*n, "Library"),
        CloseCode::Bad(n) => (*n, "Bad"),
    };
    format!("{} ({})", num, label)
}

#[cfg(test)]
mod tests {
    use super::BinanceWsClient;

    #[test]
    fn trade_stream_builds_lowercase_trade_topic() {
        assert_eq!(
            BinanceWsClient::trade_stream("BTCUSDT"),
            "btcusdt@trade".to_string()
        );
        assert_eq!(
            BinanceWsClient::trade_stream("ethusdt"),
            "ethusdt@trade".to_string()
        );
    }
}
