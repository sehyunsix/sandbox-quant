use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use super::types::BinanceTradeEvent;
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
    pub fn new(ws_base_url: &str, stream: &str) -> Self {
        Self {
            url: format!("{}/{}", ws_base_url, stream),
        }
    }

    /// Connect and run the WebSocket loop with automatic reconnection.
    pub async fn connect_and_run(
        &self,
        tick_tx: mpsc::Sender<Tick>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<()> {
        let mut backoff = ExponentialBackoff::new(
            Duration::from_secs(1),
            Duration::from_secs(60),
            2.0,
        );

        loop {
            match self.connect_once(&tick_tx, &mut shutdown).await {
                Ok(()) => {
                    tracing::info!("WebSocket closed cleanly (shutdown)");
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "WebSocket disconnected");
                    let delay = backoff.next_delay();
                    tracing::info!(delay_ms = delay.as_millis() as u64, "Reconnecting...");

                    tokio::select! {
                        _ = tokio::time::sleep(delay) => continue,
                        _ = shutdown.changed() => {
                            tracing::info!("Shutdown during reconnect backoff");
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
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        tracing::info!(url = %self.url, "Connecting to WebSocket");

        let (ws_stream, _resp) = tokio_tungstenite::connect_async(&self.url)
            .await
            .context("WebSocket connect failed")?;

        tracing::info!("WebSocket connected");

        let (_write, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(tungstenite::Message::Text(text))) => {
                            match serde_json::from_str::<BinanceTradeEvent>(&text) {
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
                                    tracing::debug!(error = %e, raw = %text, "Failed to parse WS message");
                                }
                            }
                        }
                        Some(Ok(tungstenite::Message::Ping(_))) => {
                            // tokio-tungstenite handles pong automatically
                        }
                        Some(Ok(_)) => {
                            // Binary, Pong, Close, Frame - ignore
                        }
                        Some(Err(e)) => {
                            return Err(anyhow::anyhow!("WebSocket read error: {}", e));
                        }
                        None => {
                            return Err(anyhow::anyhow!("WebSocket stream ended"));
                        }
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("Shutdown signal received, closing WebSocket");
                    return Ok(());
                }
            }
        }
    }
}

use tokio_tungstenite::tungstenite;
