use anyhow::{bail, Context, Result};
use chrono::DateTime;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::Mutex;

use crate::config::AlpacaAssetClass;
use crate::model::candle::Candle;
use crate::model::tick::Tick;

#[derive(Debug, Clone)]
pub struct OptionChainRow {
    pub symbol: String,
    pub strike: Option<f64>,
    pub option_type: String,
    pub theoretical_price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub delta: Option<f64>,
    pub theta: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct OptionChainSnapshot {
    pub underlying: String,
    pub rows: Vec<OptionChainRow>,
    pub status: Option<String>,
}

pub struct AlpacaRestClient {
    http: reqwest::Client,
    trading_base_url: String,
    data_base_url: String,
    option_snapshot_feeds: Vec<String>,
    option_symbol_cache: Mutex<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct AlpacaOrderAck {
    pub id: String,
    pub status: String,
    pub qty: Option<f64>,
    pub filled_avg_price: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AlpacaOrderResponse {
    id: String,
    status: String,
    #[serde(default)]
    filled_avg_price: Option<String>,
    #[serde(default)]
    qty: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlpacaPositionResponse {
    qty: String,
}

impl AlpacaRestClient {
    pub fn new(
        trading_base_url: &str,
        data_base_url: &str,
        api_key: &str,
        api_secret: &str,
        option_snapshot_feeds: &[String],
    ) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("APCA-API-KEY-ID", HeaderValue::from_str(api_key)?);
        headers.insert("APCA-API-SECRET-KEY", HeaderValue::from_str(api_secret)?);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build Alpaca HTTP client")?;
        Ok(Self {
            http,
            trading_base_url: trading_base_url.to_string(),
            data_base_url: data_base_url.to_string(),
            option_snapshot_feeds: option_snapshot_feeds.to_vec(),
            option_symbol_cache: Mutex::new(HashMap::new()),
        })
    }

    fn compact_error_body(body: &str) -> String {
        let normalized = body.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.len() > 180 {
            format!("{}...", &normalized[..180])
        } else {
            normalized
        }
    }

    fn looks_like_option_symbol(symbol: &str) -> bool {
        // OPRA format is typically <UNDERLYING><YYMMDD><C/P><8-digit strike>.
        // Example: AAPL240621C00190000
        let trimmed = symbol.trim().to_ascii_uppercase();
        if trimmed.len() < 16 {
            return false;
        }
        let mut chars = trimmed.chars().rev();
        let suffix: String = chars
            .by_ref()
            .take(15)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let bytes = suffix.as_bytes();
        if bytes.len() != 15 {
            return false;
        }
        bytes[0..6].iter().all(|b| b.is_ascii_digit())
            && (bytes[6] == b'C' || bytes[6] == b'P')
            && bytes[7..15].iter().all(|b| b.is_ascii_digit())
    }

    fn option_underlying_symbol(symbol: &str) -> String {
        let sym = symbol.trim().to_ascii_uppercase();
        if Self::looks_like_option_symbol(&sym) {
            sym[..sym.len().saturating_sub(15)].to_string()
        } else {
            sym
        }
    }

    fn option_type_from_symbol(symbol: &str) -> String {
        symbol
            .as_bytes()
            .get(symbol.len().saturating_sub(9))
            .map(|b| {
                if *b == b'C' {
                    "CALL"
                } else if *b == b'P' {
                    "PUT"
                } else {
                    "UNKNOWN"
                }
            })
            .unwrap_or("UNKNOWN")
            .to_string()
    }

    fn strike_from_symbol(symbol: &str) -> Option<f64> {
        let bytes = symbol.as_bytes();
        if bytes.len() < 8 {
            return None;
        }
        let strike_str = &symbol[symbol.len() - 8..];
        strike_str.parse::<u64>().ok().map(|v| v as f64 / 1000.0)
    }

    async fn resolve_option_symbol(&self, symbol: &str) -> Result<String> {
        if Self::looks_like_option_symbol(symbol) {
            return Ok(symbol.to_ascii_uppercase());
        }
        let underlying = symbol.trim().to_ascii_uppercase();
        if underlying.is_empty() {
            bail!("option symbol is empty");
        }

        if let Some(cached) = self
            .option_symbol_cache
            .lock()
            .await
            .get(&underlying)
            .cloned()
        {
            return Ok(cached);
        }

        let endpoint = format!("{}/v2/options/contracts", self.trading_base_url);
        let root: Value = self
            .http
            .get(&endpoint)
            .query(&[
                ("underlying_symbols", underlying.as_str()),
                ("status", "active"),
                ("limit", "1"),
                ("type", "call"),
            ])
            .send()
            .await
            .context("alpaca resolve option contract HTTP failed")?
            .error_for_status()
            .context("alpaca resolve option contract returned error status")?
            .json()
            .await
            .context("alpaca resolve option contract JSON parse failed")?;

        let contracts = root
            .get("option_contracts")
            .or_else(|| root.get("contracts"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let Some(contract_symbol) = contracts
            .first()
            .and_then(|c| c.get("symbol"))
            .and_then(Value::as_str)
            .map(|s| s.to_ascii_uppercase())
        else {
            bail!(
                "no active option contract found for underlying {} (set [alpaca].symbol to an option contract)",
                underlying
            );
        };

        self.option_symbol_cache
            .lock()
            .await
            .insert(underlying, contract_symbol.clone());
        Ok(contract_symbol)
    }

    pub async fn resolve_symbol_for_asset(
        &self,
        symbol: &str,
        asset_class: AlpacaAssetClass,
    ) -> Result<String> {
        match asset_class {
            AlpacaAssetClass::UsOption => self.resolve_option_symbol(symbol).await,
            _ => Ok(symbol.to_ascii_uppercase()),
        }
    }

    pub async fn ping(&self) -> Result<()> {
        let url = format!("{}/v2/account", self.trading_base_url);
        self.http
            .get(&url)
            .send()
            .await
            .context("alpaca ping failed")?
            .error_for_status()
            .context("alpaca ping returned error status")?;
        Ok(())
    }

    pub async fn get_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
        asset_class: AlpacaAssetClass,
    ) -> Result<Vec<Candle>> {
        if matches!(asset_class, AlpacaAssetClass::UsFuture) {
            return Ok(Vec::new());
        }
        let timeframe = match interval {
            "1s" => "1Sec",
            "1m" => "1Min",
            "1h" => "1Hour",
            "1d" => "1Day",
            _ => "1Min",
        };

        let request_symbol = self.resolve_symbol_for_asset(symbol, asset_class).await?;
        let endpoint = match asset_class {
            AlpacaAssetClass::UsEquity => format!("{}/v2/stocks/bars", self.data_base_url),
            AlpacaAssetClass::UsOption => format!("{}/v1beta1/options/bars", self.data_base_url),
            AlpacaAssetClass::UsFuture => unreachable!(),
        };

        let limit = limit.clamp(1, 10_000);
        let limit_s = limit.to_string();
        let mut request = self.http.get(&endpoint).query(&[
            ("symbols", request_symbol.as_str()),
            ("timeframe", timeframe),
            ("limit", limit_s.as_str()),
            // Request latest N bars first, then re-sort ascending below for chart rendering.
            ("sort", "desc"),
        ]);
        if matches!(asset_class, AlpacaAssetClass::UsEquity) {
            // Paper accounts typically have IEX access; forcing this avoids sparse/empty windows.
            request = request.query(&[("feed", "iex")]);
        }

        let root: Value = request
            .send()
            .await
            .context("alpaca get_klines HTTP failed")?
            .error_for_status()
            .context("alpaca get_klines returned error status")?
            .json()
            .await
            .context("alpaca get_klines JSON parse failed")?;

        let bars = root
            .get("bars")
            .and_then(|b| b.get(request_symbol.as_str()))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut candles = Vec::with_capacity(bars.len());
        for bar in bars {
            let open_time_str = bar.get("t").and_then(Value::as_str).unwrap_or_default();
            let open_time = parse_rfc3339_ms(open_time_str)?;
            let open = bar.get("o").and_then(Value::as_f64).unwrap_or(0.0);
            let high = bar.get("h").and_then(Value::as_f64).unwrap_or(open);
            let low = bar.get("l").and_then(Value::as_f64).unwrap_or(open);
            let close = bar.get("c").and_then(Value::as_f64).unwrap_or(open);
            let close_time = open_time.saturating_add(interval_to_ms(interval));
            candles.push(Candle {
                open,
                high,
                low,
                close,
                open_time,
                close_time,
            });
        }
        candles.sort_by_key(|c| c.open_time);
        Ok(candles)
    }

    pub async fn get_latest_trade(
        &self,
        symbol: &str,
        asset_class: AlpacaAssetClass,
    ) -> Result<Option<Tick>> {
        let request_symbol = self.resolve_symbol_for_asset(symbol, asset_class).await?;
        let url = match asset_class {
            AlpacaAssetClass::UsEquity => format!(
                "{}/v2/stocks/trades/latest?symbols={}",
                self.data_base_url, request_symbol
            ),
            AlpacaAssetClass::UsOption => format!(
                "{}/v1beta1/options/trades/latest?symbols={}",
                self.data_base_url, request_symbol
            ),
            AlpacaAssetClass::UsFuture => {
                return Ok(None);
            }
        };

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .context("alpaca latest trade HTTP failed")?;
        if !response.status().is_success() {
            // Non-success can happen due permission/symbol mismatches.
            // Keep returning None but emit a concise warning for triage.
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(
                status = %status,
                symbol = %request_symbol,
                detail = %Self::compact_error_body(&body),
                "alpaca latest trade returned non-success"
            );
            return Ok(None);
        }
        let root: Value = response
            .json()
            .await
            .context("alpaca latest trade JSON parse failed")?;

        let trade = root
            .get("trades")
            .and_then(|t| t.get(request_symbol.as_str()))
            .or_else(|| root.get("trade"));

        let Some(trade) = trade else {
            return Ok(None);
        };

        let price = trade.get("p").and_then(Value::as_f64).unwrap_or(0.0);
        if price <= 0.0 {
            return Ok(None);
        }
        let qty = trade.get("s").and_then(Value::as_f64).unwrap_or(0.0);
        let ts = trade.get("t").and_then(Value::as_str).unwrap_or_default();
        let timestamp_ms = parse_rfc3339_ms(ts)?;
        Ok(Some(Tick {
            price,
            qty,
            timestamp_ms,
            is_buyer_maker: false,
            trade_id: 0,
        }))
    }

    pub async fn get_option_chain_snapshot(
        &self,
        symbol: &str,
        max_rows: usize,
    ) -> Result<Option<OptionChainSnapshot>> {
        let underlying = Self::option_underlying_symbol(symbol);
        if underlying.is_empty() {
            return Ok(None);
        }

        let snapshots_url = format!(
            "{}/v1beta1/options/snapshots/{}",
            self.data_base_url, underlying
        );
        let mut rows: Vec<OptionChainRow> = Vec::new();
        let mut status_notes: Vec<String> = Vec::new();

        for feed in &self.option_snapshot_feeds {
            let response = self
                .http
                .get(&snapshots_url)
                .query(&[("feed", feed.as_str())])
                .send()
                .await
                .context("alpaca option chain HTTP failed")?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                status_notes.push(format!(
                    "feed={} status={} {}",
                    feed,
                    status,
                    Self::compact_error_body(&body)
                ));
                continue;
            }

            let snapshots_root: Value = match response.json().await {
                Ok(v) => v,
                Err(e) => {
                    status_notes.push(format!("feed={} invalid JSON: {}", feed, e));
                    continue;
                }
            };

            let snapshots_obj = snapshots_root
                .get("snapshots")
                .and_then(Value::as_object)
                .or_else(|| snapshots_root.as_object());
            let Some(snapshots) = snapshots_obj else {
                status_notes.push(format!("feed={} snapshot payload missing", feed));
                continue;
            };

            rows = snapshots
                .iter()
                .filter(|(contract_symbol, _)| Self::looks_like_option_symbol(contract_symbol))
                .map(|(contract_symbol, snapshot)| {
                    let latest_quote = snapshot
                        .get("latestQuote")
                        .or_else(|| snapshot.get("latest_quote"));
                    let greeks = snapshot.get("greeks");
                    let model = snapshot.get("model");

                    let bid = latest_quote
                        .and_then(|q| q.get("bp").or_else(|| q.get("bid_price")))
                        .and_then(Value::as_f64);
                    let ask = latest_quote
                        .and_then(|q| q.get("ap").or_else(|| q.get("ask_price")))
                        .and_then(Value::as_f64);
                    let delta = greeks.and_then(|g| g.get("delta")).and_then(Value::as_f64);
                    let theta = greeks.and_then(|g| g.get("theta")).and_then(Value::as_f64);
                    let theoretical_price = model
                        .and_then(|m| m.get("theoretical_price").or_else(|| m.get("theo")))
                        .and_then(Value::as_f64);

                    OptionChainRow {
                        symbol: contract_symbol.to_ascii_uppercase(),
                        strike: Self::strike_from_symbol(contract_symbol),
                        option_type: Self::option_type_from_symbol(contract_symbol),
                        theoretical_price,
                        bid,
                        ask,
                        delta,
                        theta,
                    }
                })
                .collect();

            if rows.is_empty() {
                status_notes.push(format!("feed={} returned empty snapshot", feed));
                continue;
            }
            if feed != "opra" {
                status_notes.push(format!("snapshot feed={}", feed));
            }
            break;
        }

        if rows.is_empty() {
            // Fallback: at least render contract strikes/types even when snapshots are unavailable.
            let contract_limit = max_rows.clamp(1, 200).to_string();
            let contracts_url = format!("{}/v2/options/contracts", self.trading_base_url);
            let contracts_root: Value = match self
                .http
                .get(&contracts_url)
                .query(&[
                    ("underlying_symbols", underlying.as_str()),
                    ("status", "active"),
                    ("limit", contract_limit.as_str()),
                ])
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => resp.json().await.unwrap_or(Value::Null),
                _ => Value::Null,
            };

            if let Some(arr) = contracts_root
                .get("option_contracts")
                .or_else(|| contracts_root.get("contracts"))
                .and_then(Value::as_array)
            {
                rows = arr
                    .iter()
                    .filter_map(|c| c.get("symbol").and_then(Value::as_str))
                    .map(|s| OptionChainRow {
                        symbol: s.to_ascii_uppercase(),
                        strike: Self::strike_from_symbol(s),
                        option_type: Self::option_type_from_symbol(s),
                        theoretical_price: None,
                        bid: None,
                        ask: None,
                        delta: None,
                        theta: None,
                    })
                    .collect();
            }
            if !rows.is_empty() {
                status_notes.push("quotes unavailable; showing contract list only".to_string());
            }
        }

        rows.sort_by(|a, b| {
            let sa = a.strike.unwrap_or(f64::MAX);
            let sb = b.strike.unwrap_or(f64::MAX);
            sa.partial_cmp(&sb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.option_type.cmp(&b.option_type))
        });
        let capped = max_rows.max(1);
        if rows.len() > capped {
            rows.truncate(capped);
        }

        let status = if status_notes.is_empty() {
            None
        } else {
            Some(status_notes.join(" | "))
        };
        Ok(Some(OptionChainSnapshot {
            underlying,
            rows,
            status,
        }))
    }

    pub async fn get_position_qty(&self, symbol: &str) -> Result<f64> {
        let url = format!("{}/v2/positions/{}", self.trading_base_url, symbol);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .context("alpaca get position HTTP failed")?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(0.0);
        }
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("alpaca get position failed: {}", body);
        }
        let pos: AlpacaPositionResponse = response
            .json()
            .await
            .context("alpaca get position JSON parse failed")?;
        Ok(pos.qty.parse::<f64>().unwrap_or(0.0))
    }

    pub async fn place_market_order_notional(
        &self,
        symbol: &str,
        side: &str,
        notional: f64,
    ) -> Result<AlpacaOrderAck> {
        let url = format!("{}/v2/orders", self.trading_base_url);
        let body = serde_json::json!({
            "symbol": symbol,
            "side": side,
            "type": "market",
            "time_in_force": "day",
            "notional": format!("{:.2}", notional.max(1.0)),
        });
        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("alpaca place market notional order HTTP failed")?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("alpaca order rejected: {}", body);
        }
        let order: AlpacaOrderResponse = response
            .json()
            .await
            .context("alpaca order response parse failed")?;
        Ok(AlpacaOrderAck {
            id: order.id,
            status: order.status,
            qty: order.qty.and_then(|v| v.parse::<f64>().ok()),
            filled_avg_price: order.filled_avg_price.and_then(|v| v.parse::<f64>().ok()),
        })
    }

    pub async fn place_market_order_qty(
        &self,
        symbol: &str,
        side: &str,
        qty: f64,
    ) -> Result<AlpacaOrderAck> {
        let url = format!("{}/v2/orders", self.trading_base_url);
        let body = serde_json::json!({
            "symbol": symbol,
            "side": side,
            "type": "market",
            "time_in_force": "day",
            "qty": format!("{:.4}", qty.max(0.0)),
        });
        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("alpaca place market qty order HTTP failed")?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("alpaca order rejected: {}", body);
        }
        let order: AlpacaOrderResponse = response
            .json()
            .await
            .context("alpaca order response parse failed")?;
        Ok(AlpacaOrderAck {
            id: order.id,
            status: order.status,
            qty: order.qty.and_then(|v| v.parse::<f64>().ok()),
            filled_avg_price: order.filled_avg_price.and_then(|v| v.parse::<f64>().ok()),
        })
    }
}

fn parse_rfc3339_ms(s: &str) -> Result<u64> {
    let dt =
        DateTime::parse_from_rfc3339(s).with_context(|| format!("invalid timestamp '{}'", s))?;
    Ok(dt.timestamp_millis().max(0) as u64)
}

fn interval_to_ms(interval: &str) -> u64 {
    match interval {
        "1s" => 1_000,
        "1m" => 60_000,
        "1h" => 3_600_000,
        "1d" => 86_400_000,
        _ => 60_000,
    }
}
