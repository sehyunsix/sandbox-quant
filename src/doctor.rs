use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::time::Instant;

use crate::binance::rest::BinanceRestClient;
use crate::binance::types::BinanceFuturesPositionRisk;
use crate::config::Config;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorCommand {
    Help,
    Auth {
        json: bool,
    },
    Positions {
        market: String,
        symbol: Option<String>,
        json: bool,
    },
    Pnl {
        market: String,
        symbol: Option<String>,
        json: bool,
    },
    History {
        market: String,
        symbol: Option<String>,
        json: bool,
    },
    SyncOnce {
        market: String,
        symbol: Option<String>,
        json: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
struct DoctorEnvelope<T: Serialize> {
    status: DoctorStatus,
    timestamp_ms: u64,
    command: String,
    data: T,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AuthCheckRow {
    endpoint: String,
    ok: bool,
    code: Option<i64>,
    message: String,
}

#[derive(Debug, Serialize)]
struct AuthReport {
    rest_base_url: String,
    futures_rest_base_url: String,
    spot_key_len: usize,
    spot_secret_len: usize,
    futures_key_len: usize,
    futures_secret_len: usize,
    checks: Vec<AuthCheckRow>,
}

#[derive(Debug, Clone, Serialize)]
struct PositionRow {
    symbol: String,
    side: String,
    qty_abs: f64,
    qty_signed: f64,
    entry_price: f64,
    mark_price: f64,
    unrealized_api: f64,
    unrealized_final: f64,
    selected_source: String,
}

#[derive(Debug, Serialize)]
struct PositionsReport {
    market: String,
    symbol_filter: Option<String>,
    count: usize,
    rows: Vec<PositionRow>,
}

#[derive(Debug, Serialize)]
struct PnlRow {
    symbol: String,
    side: String,
    qty_signed: f64,
    entry_price: f64,
    mark_price: f64,
    api_unrealized: f64,
    fallback_unrealized: f64,
    final_unrealized: f64,
    selected_source: String,
}

#[derive(Debug, Serialize)]
struct PnlReport {
    market: String,
    symbol_filter: Option<String>,
    count: usize,
    rows: Vec<PnlRow>,
}

#[derive(Debug, Serialize)]
struct HistoryReport {
    market: String,
    symbol: String,
    all_orders_ok: bool,
    trades_ok: bool,
    all_orders_count: usize,
    trades_count: usize,
    latest_order_ms: Option<u64>,
    latest_trade_ms: Option<u64>,
    fetch_latency_ms: u64,
}

#[derive(Debug, Serialize)]
struct SyncOnceReport {
    market: String,
    symbol: String,
    auth_spot_ok: bool,
    auth_futures_ok: bool,
    futures_positions_count: usize,
    history_all_orders_ok: bool,
    history_trades_ok: bool,
    history_all_orders_count: usize,
    history_trades_count: usize,
    total_latency_ms: u64,
}

pub fn parse_doctor_args(args: &[String]) -> Result<Option<DoctorCommand>> {
    if args.len() < 2 || args[1] != "doctor" {
        return Ok(None);
    }
    if args.len() == 2 {
        return Ok(Some(DoctorCommand::Help));
    }
    let sub = args.get(2).map(|s| s.as_str()).unwrap_or("");
    if sub == "help" || sub == "--help" || sub == "-h" {
        return Ok(Some(DoctorCommand::Help));
    }
    let mut json = false;
    let mut market = "futures".to_string();
    let mut symbol: Option<String> = None;
    let mut once = false;

    let mut i = 3usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json = true;
                i += 1;
            }
            "--once" => {
                once = true;
                i += 1;
            }
            "--help" | "-h" => {
                return Ok(Some(DoctorCommand::Help));
            }
            "--market" => {
                let v = args
                    .get(i + 1)
                    .with_context(|| "--market requires a value")?
                    .trim()
                    .to_ascii_lowercase();
                market = v;
                i += 2;
            }
            "--symbol" => {
                let v = args
                    .get(i + 1)
                    .with_context(|| "--symbol requires a value")?
                    .trim()
                    .to_ascii_uppercase();
                symbol = Some(v);
                i += 2;
            }
            unknown => bail!("unknown doctor option: {}", unknown),
        }
    }

    let cmd = match sub {
        "auth" => DoctorCommand::Auth { json },
        "positions" => DoctorCommand::Positions {
            market,
            symbol,
            json,
        },
        "pnl" => DoctorCommand::Pnl {
            market,
            symbol,
            json,
        },
        "history" => DoctorCommand::History {
            market,
            symbol,
            json,
        },
        "sync" => {
            if !once {
                bail!("doctor sync currently requires --once");
            }
            DoctorCommand::SyncOnce {
                market,
                symbol,
                json,
            }
        }
        _ => {
            bail!(
                "unknown doctor subcommand: '{}'. expected one of: auth, positions, pnl, history, sync",
                sub
            )
        }
    };
    Ok(Some(cmd))
}

pub async fn maybe_run_doctor_from_args(args: &[String]) -> Result<bool> {
    let Some(cmd) = parse_doctor_args(args)? else {
        return Ok(false);
    };
    run_doctor(cmd).await?;
    Ok(true)
}

async fn run_doctor(cmd: DoctorCommand) -> Result<()> {
    if matches!(cmd, DoctorCommand::Help) {
        print_doctor_help();
        return Ok(());
    }

    let cfg = Config::load()?;
    let client = BinanceRestClient::new(
        &cfg.binance.rest_base_url,
        &cfg.binance.futures_rest_base_url,
        &cfg.binance.api_key,
        &cfg.binance.api_secret,
        &cfg.binance.futures_api_key,
        &cfg.binance.futures_api_secret,
        cfg.binance.recv_window,
    );

    match cmd {
        DoctorCommand::Help => {}
        DoctorCommand::Auth { json } => {
            let report = run_auth(&cfg, &client).await;
            if json {
                print_json_envelope("doctor auth", report.status, report.data, report.errors)?;
            } else {
                print_auth_text(&report.data, &report.errors);
            }
        }
        DoctorCommand::Positions {
            market,
            symbol,
            json,
        } => {
            if market != "futures" {
                return Err(anyhow!(
                    "doctor positions currently supports only --market futures"
                ));
            }
            let report = run_positions(&client, symbol.clone()).await;
            if json {
                print_json_envelope(
                    "doctor positions",
                    report.status,
                    report.data,
                    report.errors,
                )?;
            } else {
                print_positions_text(&report.data, &report.errors);
            }
        }
        DoctorCommand::Pnl {
            market,
            symbol,
            json,
        } => {
            if market != "futures" {
                return Err(anyhow!(
                    "doctor pnl currently supports only --market futures"
                ));
            }
            let report = run_pnl(&client, symbol.clone()).await;
            if json {
                print_json_envelope("doctor pnl", report.status, report.data, report.errors)?;
            } else {
                print_pnl_text(&report.data, &report.errors);
            }
        }
        DoctorCommand::History {
            market,
            symbol,
            json,
        } => {
            let symbol = normalize_symbol_for_market(symbol, &market, &cfg.binance.symbol)?;
            let report = run_history(&client, &market, symbol).await;
            if json {
                print_json_envelope("doctor history", report.status, report.data, report.errors)?;
            } else {
                print_history_text(&report.data, &report.errors);
            }
        }
        DoctorCommand::SyncOnce {
            market,
            symbol,
            json,
        } => {
            let symbol = normalize_symbol_for_market(symbol, &market, &cfg.binance.symbol)?;
            let report = run_sync_once(&client, &market, symbol).await;
            if json {
                print_json_envelope(
                    "doctor sync --once",
                    report.status,
                    report.data,
                    report.errors,
                )?;
            } else {
                print_sync_once_text(&report.data, &report.errors);
            }
        }
    }

    Ok(())
}

fn normalize_symbol_for_market(
    symbol: Option<String>,
    market: &str,
    default_symbol: &str,
) -> Result<String> {
    let raw = symbol.unwrap_or_else(|| default_symbol.to_ascii_uppercase());
    let s = raw.trim().to_ascii_uppercase();
    if s.is_empty() {
        bail!("symbol must not be empty");
    }
    let base = s
        .trim_end_matches(" (FUT)")
        .trim_end_matches("#FUT")
        .trim()
        .to_string();
    if base.is_empty() {
        bail!("symbol must not be empty after normalization");
    }
    match market {
        "spot" | "futures" => Ok(base),
        _ => bail!("unsupported market '{}', expected spot or futures", market),
    }
}

struct ReportOutcome<T> {
    status: DoctorStatus,
    data: T,
    errors: Vec<String>,
}

async fn run_auth(cfg: &Config, client: &BinanceRestClient) -> ReportOutcome<AuthReport> {
    let mut checks = Vec::new();
    let mut errors = Vec::new();

    match client.get_account().await {
        Ok(_) => checks.push(AuthCheckRow {
            endpoint: "spot:/api/v3/account".to_string(),
            ok: true,
            code: None,
            message: "OK".to_string(),
        }),
        Err(e) => {
            let (code, msg) = extract_error_code_message(&e);
            checks.push(AuthCheckRow {
                endpoint: "spot:/api/v3/account".to_string(),
                ok: false,
                code,
                message: msg.clone(),
            });
            errors.push(format!("spot auth failed: {}", msg));
        }
    }

    match client.get_futures_account().await {
        Ok(_) => checks.push(AuthCheckRow {
            endpoint: "futures:/fapi/v2/account".to_string(),
            ok: true,
            code: None,
            message: "OK".to_string(),
        }),
        Err(e) => {
            let (code, msg) = extract_error_code_message(&e);
            checks.push(AuthCheckRow {
                endpoint: "futures:/fapi/v2/account".to_string(),
                ok: false,
                code,
                message: msg.clone(),
            });
            errors.push(format!("futures auth failed: {}", msg));
        }
    }

    let status = if errors.is_empty() {
        DoctorStatus::Ok
    } else {
        DoctorStatus::Fail
    };

    ReportOutcome {
        status,
        data: AuthReport {
            rest_base_url: cfg.binance.rest_base_url.clone(),
            futures_rest_base_url: cfg.binance.futures_rest_base_url.clone(),
            spot_key_len: cfg.binance.api_key.len(),
            spot_secret_len: cfg.binance.api_secret.len(),
            futures_key_len: cfg.binance.futures_api_key.len(),
            futures_secret_len: cfg.binance.futures_api_secret.len(),
            checks,
        },
        errors,
    }
}

async fn run_positions(
    client: &BinanceRestClient,
    symbol_filter: Option<String>,
) -> ReportOutcome<PositionsReport> {
    match client.get_futures_position_risk().await {
        Ok(rows) => {
            let mapped = map_positions(rows, symbol_filter.clone());
            let status = if mapped.is_empty() {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Ok
            };
            ReportOutcome {
                status,
                data: PositionsReport {
                    market: "futures".to_string(),
                    symbol_filter,
                    count: mapped.len(),
                    rows: mapped,
                },
                errors: Vec::new(),
            }
        }
        Err(e) => {
            let (_code, msg) = extract_error_code_message(&e);
            ReportOutcome {
                status: DoctorStatus::Fail,
                data: PositionsReport {
                    market: "futures".to_string(),
                    symbol_filter,
                    count: 0,
                    rows: Vec::new(),
                },
                errors: vec![msg],
            }
        }
    }
}

async fn run_pnl(
    client: &BinanceRestClient,
    symbol_filter: Option<String>,
) -> ReportOutcome<PnlReport> {
    match client.get_futures_position_risk().await {
        Ok(rows) => {
            let mut out = Vec::new();
            for row in rows {
                if row.position_amt.abs() <= f64::EPSILON {
                    continue;
                }
                if let Some(filter) = symbol_filter.as_ref() {
                    if row.symbol.trim().to_ascii_uppercase() != *filter {
                        continue;
                    }
                }
                let (final_unr, source) = resolve_unrealized_pnl(
                    row.unrealized_profit,
                    row.mark_price,
                    row.entry_price,
                    row.position_amt,
                );
                let fallback = if row.mark_price > f64::EPSILON && row.entry_price > f64::EPSILON {
                    (row.mark_price - row.entry_price) * row.position_amt
                } else {
                    0.0
                };
                out.push(PnlRow {
                    symbol: format!("{} (FUT)", row.symbol.trim().to_ascii_uppercase()),
                    side: side_text(row.position_amt).to_string(),
                    qty_signed: row.position_amt,
                    entry_price: row.entry_price,
                    mark_price: row.mark_price,
                    api_unrealized: row.unrealized_profit,
                    fallback_unrealized: fallback,
                    final_unrealized: final_unr,
                    selected_source: source.to_string(),
                });
            }
            let status = if out.is_empty() {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Ok
            };
            ReportOutcome {
                status,
                data: PnlReport {
                    market: "futures".to_string(),
                    symbol_filter,
                    count: out.len(),
                    rows: out,
                },
                errors: Vec::new(),
            }
        }
        Err(e) => {
            let (_code, msg) = extract_error_code_message(&e);
            ReportOutcome {
                status: DoctorStatus::Fail,
                data: PnlReport {
                    market: "futures".to_string(),
                    symbol_filter,
                    count: 0,
                    rows: Vec::new(),
                },
                errors: vec![msg],
            }
        }
    }
}

async fn run_history(
    client: &BinanceRestClient,
    market: &str,
    symbol: String,
) -> ReportOutcome<HistoryReport> {
    let started = Instant::now();
    let (
        orders_ok,
        orders_count,
        latest_order_ms,
        orders_err,
        trades_ok,
        trades_count,
        latest_trade_ms,
        trades_err,
    ) = if market == "futures" {
        let orders = client.get_futures_all_orders(&symbol, 1000).await;
        let trades = client.get_futures_my_trades_history(&symbol, 1000).await;
        (
            orders.is_ok(),
            orders.as_ref().map(|v| v.len()).unwrap_or(0),
            orders
                .as_ref()
                .ok()
                .and_then(|v| v.iter().map(|o| o.update_time.max(o.time)).max()),
            orders.err().map(|e| e.to_string()),
            trades.is_ok(),
            trades.as_ref().map(|v| v.len()).unwrap_or(0),
            trades
                .as_ref()
                .ok()
                .and_then(|v| v.iter().map(|t| t.time).max()),
            trades.err().map(|e| e.to_string()),
        )
    } else {
        let orders = client.get_all_orders(&symbol, 1000).await;
        let trades = client.get_my_trades_history(&symbol, 1000).await;
        (
            orders.is_ok(),
            orders.as_ref().map(|v| v.len()).unwrap_or(0),
            orders
                .as_ref()
                .ok()
                .and_then(|v| v.iter().map(|o| o.update_time.max(o.time)).max()),
            orders.err().map(|e| e.to_string()),
            trades.is_ok(),
            trades.as_ref().map(|v| v.len()).unwrap_or(0),
            trades
                .as_ref()
                .ok()
                .and_then(|v| v.iter().map(|t| t.time).max()),
            trades.err().map(|e| e.to_string()),
        )
    };

    let mut errors = Vec::new();
    if let Some(e) = orders_err {
        errors.push(format!("allOrders failed: {}", e));
    }
    if let Some(e) = trades_err {
        errors.push(format!("trades failed: {}", e));
    }
    let status = if orders_ok && trades_ok {
        DoctorStatus::Ok
    } else if orders_ok || trades_ok {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Fail
    };
    ReportOutcome {
        status,
        data: HistoryReport {
            market: market.to_string(),
            symbol,
            all_orders_ok: orders_ok,
            trades_ok,
            all_orders_count: orders_count,
            trades_count,
            latest_order_ms,
            latest_trade_ms,
            fetch_latency_ms: started.elapsed().as_millis() as u64,
        },
        errors,
    }
}

async fn run_sync_once(
    client: &BinanceRestClient,
    market: &str,
    symbol: String,
) -> ReportOutcome<SyncOnceReport> {
    let started = Instant::now();
    let spot_auth = client.get_account().await;
    let futures_auth = client.get_futures_account().await;
    let futures_positions = client.get_futures_position_risk().await;
    let history = run_history(client, market, symbol.clone()).await;

    let mut errors = Vec::new();
    if let Err(e) = spot_auth.as_ref() {
        errors.push(format!("spot auth failed: {}", e));
    }
    if let Err(e) = futures_auth.as_ref() {
        errors.push(format!("futures auth failed: {}", e));
    }
    if let Err(e) = futures_positions.as_ref() {
        errors.push(format!("futures positions failed: {}", e));
    }
    errors.extend(history.errors.clone());

    let status = if errors.is_empty() {
        DoctorStatus::Ok
    } else if spot_auth.is_ok()
        || futures_auth.is_ok()
        || history.data.all_orders_ok
        || history.data.trades_ok
    {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Fail
    };

    let futures_positions_count = futures_positions
        .as_ref()
        .ok()
        .map(|rows| {
            rows.iter()
                .filter(|p| p.position_amt.abs() > f64::EPSILON)
                .count()
        })
        .unwrap_or(0);

    ReportOutcome {
        status,
        data: SyncOnceReport {
            market: market.to_string(),
            symbol,
            auth_spot_ok: spot_auth.is_ok(),
            auth_futures_ok: futures_auth.is_ok(),
            futures_positions_count,
            history_all_orders_ok: history.data.all_orders_ok,
            history_trades_ok: history.data.trades_ok,
            history_all_orders_count: history.data.all_orders_count,
            history_trades_count: history.data.trades_count,
            total_latency_ms: started.elapsed().as_millis() as u64,
        },
        errors,
    }
}

fn map_positions(
    rows: Vec<BinanceFuturesPositionRisk>,
    symbol_filter: Option<String>,
) -> Vec<PositionRow> {
    let mut out = Vec::new();
    for row in rows {
        if row.position_amt.abs() <= f64::EPSILON {
            continue;
        }
        if let Some(filter) = symbol_filter.as_ref() {
            if row.symbol.trim().to_ascii_uppercase() != *filter {
                continue;
            }
        }
        let (unrealized_final, selected_source) = resolve_unrealized_pnl(
            row.unrealized_profit,
            row.mark_price,
            row.entry_price,
            row.position_amt,
        );
        out.push(PositionRow {
            symbol: format!("{} (FUT)", row.symbol.trim().to_ascii_uppercase()),
            side: side_text(row.position_amt).to_string(),
            qty_abs: row.position_amt.abs(),
            qty_signed: row.position_amt,
            entry_price: row.entry_price,
            mark_price: row.mark_price,
            unrealized_api: row.unrealized_profit,
            unrealized_final,
            selected_source: selected_source.to_string(),
        });
    }
    out
}

pub fn resolve_unrealized_pnl(
    api_unrealized: f64,
    mark_price: f64,
    entry_price: f64,
    position_amt: f64,
) -> (f64, &'static str) {
    if api_unrealized.abs() > f64::EPSILON {
        return (api_unrealized, "api_unRealizedProfit");
    }
    if mark_price > f64::EPSILON && entry_price > f64::EPSILON && position_amt.abs() > f64::EPSILON
    {
        return (
            (mark_price - entry_price) * position_amt,
            "fallback_mark_minus_entry_times_qty",
        );
    }
    (0.0, "zero")
}

fn extract_error_code_message(err: &anyhow::Error) -> (Option<i64>, String) {
    if let Some(AppError::BinanceApi { code, msg }) = err.downcast_ref::<AppError>() {
        return (Some(*code), msg.clone());
    }
    (None, err.to_string())
}

fn side_text(qty_signed: f64) -> &'static str {
    if qty_signed > 0.0 {
        "BUY"
    } else if qty_signed < 0.0 {
        "SELL"
    } else {
        "-"
    }
}

fn print_json_envelope<T: Serialize>(
    command: &str,
    status: DoctorStatus,
    data: T,
    errors: Vec<String>,
) -> Result<()> {
    let envelope = DoctorEnvelope {
        status,
        timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        command: command.to_string(),
        data,
        errors,
    };
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}

fn print_auth_text(data: &AuthReport, errors: &[String]) {
    println!("doctor auth");
    println!(
        "hosts: rest_base_url={} futures_rest_base_url={}",
        data.rest_base_url, data.futures_rest_base_url
    );
    println!(
        "credential_lens: spot_key={} spot_secret={} futures_key={} futures_secret={}",
        data.spot_key_len, data.spot_secret_len, data.futures_key_len, data.futures_secret_len
    );
    println!("checks:");
    for c in &data.checks {
        if c.ok {
            println!("  - {}: OK", c.endpoint);
        } else {
            println!(
                "  - {}: FAIL code={} msg={}",
                c.endpoint,
                c.code
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                c.message
            );
        }
    }
    if !errors.is_empty() {
        println!("errors:");
        for e in errors {
            println!("  - {}", e);
        }
    }
}

fn print_positions_text(data: &PositionsReport, errors: &[String]) {
    println!("doctor positions --market {}", data.market);
    if let Some(s) = data.symbol_filter.as_ref() {
        println!("symbol_filter: {}", s);
    }
    println!("count: {}", data.count);
    println!("rows:");
    for r in &data.rows {
        println!(
            "  - {} side={} qty={:.6} entry={:.6} mark={:.6} api={:+.6} final={:+.6} source={}",
            r.symbol,
            r.side,
            r.qty_signed,
            r.entry_price,
            r.mark_price,
            r.unrealized_api,
            r.unrealized_final,
            r.selected_source
        );
    }
    if !errors.is_empty() {
        println!("errors:");
        for e in errors {
            println!("  - {}", e);
        }
    }
}

fn print_doctor_help() {
    println!("sandbox-quant doctor");
    println!();
    println!("Usage:");
    println!("  sandbox-quant doctor auth [--json]");
    println!("  sandbox-quant doctor positions --market futures [--symbol BTCUSDT] [--json]");
    println!("  sandbox-quant doctor pnl --market futures [--symbol BTCUSDT] [--json]");
    println!("  sandbox-quant doctor history --market spot|futures [--symbol BTCUSDT] [--json]");
    println!(
        "  sandbox-quant doctor sync --once --market spot|futures [--symbol BTCUSDT] [--json]"
    );
    println!("  sandbox-quant doctor help");
    println!();
    println!("Notes:");
    println!("  - doctor commands are read-only diagnostics");
    println!("  - --market supports spot/futures for history/sync, futures-only for positions/pnl");
}

fn print_history_text(data: &HistoryReport, errors: &[String]) {
    println!(
        "doctor history --market {} --symbol {}",
        data.market, data.symbol
    );
    println!(
        "allOrders: ok={} count={} latest_ms={}",
        data.all_orders_ok,
        data.all_orders_count,
        data.latest_order_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "trades: ok={} count={} latest_ms={}",
        data.trades_ok,
        data.trades_count,
        data.latest_trade_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("latency_ms={}", data.fetch_latency_ms);
    if !errors.is_empty() {
        println!("errors:");
        for e in errors {
            println!("  - {}", e);
        }
    }
}

fn print_sync_once_text(data: &SyncOnceReport, errors: &[String]) {
    println!(
        "doctor sync --once --market {} --symbol {}",
        data.market, data.symbol
    );
    println!(
        "auth: spot_ok={} futures_ok={}",
        data.auth_spot_ok, data.auth_futures_ok
    );
    println!("futures_positions_count={}", data.futures_positions_count);
    println!(
        "history: allOrders_ok={} trades_ok={} allOrders_count={} trades_count={}",
        data.history_all_orders_ok,
        data.history_trades_ok,
        data.history_all_orders_count,
        data.history_trades_count
    );
    println!("total_latency_ms={}", data.total_latency_ms);
    if !errors.is_empty() {
        println!("errors:");
        for e in errors {
            println!("  - {}", e);
        }
    }
}

fn print_pnl_text(data: &PnlReport, errors: &[String]) {
    println!("doctor pnl --market {}", data.market);
    if let Some(s) = data.symbol_filter.as_ref() {
        println!("symbol_filter: {}", s);
    }
    println!("count: {}", data.count);
    println!("rows:");
    for r in &data.rows {
        println!(
            "  - {} side={} qty={:.6} entry={:.6} mark={:.6} api={:+.6} fallback={:+.6} final={:+.6} source={}",
            r.symbol,
            r.side,
            r.qty_signed,
            r.entry_price,
            r.mark_price,
            r.api_unrealized,
            r.fallback_unrealized,
            r.final_unrealized,
            r.selected_source
        );
    }
    if !errors.is_empty() {
        println!("errors:");
        for e in errors {
            println!("  - {}", e);
        }
    }
}
