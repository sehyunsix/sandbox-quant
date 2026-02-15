use anyhow::Result;
use chrono::TimeZone;
use rusqlite::{params, Connection};
use std::collections::HashMap;

use crate::binance::types::{BinanceAllOrder, BinanceMyTrade};

#[derive(Debug, Clone)]
pub struct PersistedTrade {
    pub trade: BinanceMyTrade,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct DailyRealizedReturn {
    pub symbol: String,
    pub date: String,
    pub realized_return_pct: f64,
}

fn ensure_trade_schema(conn: &Connection) -> Result<()> {
    for ddl in [
        "ALTER TABLE order_history_trades ADD COLUMN commission REAL NOT NULL DEFAULT 0.0",
        "ALTER TABLE order_history_trades ADD COLUMN commission_asset TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE order_history_trades ADD COLUMN realized_pnl REAL NOT NULL DEFAULT 0.0",
    ] {
        if let Err(e) = conn.execute(ddl, []) {
            let msg = e.to_string();
            if !msg.contains("duplicate column name") {
                return Err(e.into());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryBucket {
    Day,
    Hour,
    Month,
}

pub fn persist_order_snapshot(
    symbol: &str,
    orders: &[BinanceAllOrder],
    trades: &[BinanceMyTrade],
) -> Result<()> {
    std::fs::create_dir_all("data")?;
    let mut conn = Connection::open("data/order_history.sqlite")?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS order_history_orders (
            symbol TEXT NOT NULL,
            order_id INTEGER NOT NULL,
            client_order_id TEXT NOT NULL,
            status TEXT NOT NULL,
            side TEXT NOT NULL,
            orig_qty REAL NOT NULL,
            executed_qty REAL NOT NULL,
            avg_price REAL NOT NULL,
            event_time_ms INTEGER NOT NULL,
            source TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(symbol, order_id)
        );

        CREATE TABLE IF NOT EXISTS order_history_trades (
            symbol TEXT NOT NULL,
            trade_id INTEGER NOT NULL,
            order_id INTEGER NOT NULL,
            side TEXT NOT NULL,
            qty REAL NOT NULL,
            price REAL NOT NULL,
            commission REAL NOT NULL DEFAULT 0.0,
            commission_asset TEXT NOT NULL DEFAULT '',
            event_time_ms INTEGER NOT NULL,
            realized_pnl REAL NOT NULL DEFAULT 0.0,
            source TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(symbol, trade_id)
        );
        "#,
    )?;
    ensure_trade_schema(&conn)?;

    let now_ms = chrono::Utc::now().timestamp_millis();
    let tx = conn.transaction()?;
    let mut source_by_order_id = std::collections::HashMap::new();

    for o in orders {
        let avg_price = if o.executed_qty > 0.0 {
            o.cummulative_quote_qty / o.executed_qty
        } else {
            o.price
        };
        tx.execute(
            r#"
            INSERT INTO order_history_orders (
                symbol, order_id, client_order_id, status, side,
                orig_qty, executed_qty, avg_price, event_time_ms, source, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(symbol, order_id) DO UPDATE SET
                client_order_id = excluded.client_order_id,
                status = excluded.status,
                side = excluded.side,
                orig_qty = excluded.orig_qty,
                executed_qty = excluded.executed_qty,
                avg_price = excluded.avg_price,
                event_time_ms = excluded.event_time_ms,
                source = excluded.source,
                updated_at_ms = excluded.updated_at_ms
            "#,
            params![
                symbol,
                o.order_id as i64,
                o.client_order_id,
                o.status,
                o.side,
                o.orig_qty,
                o.executed_qty,
                avg_price,
                o.update_time.max(o.time) as i64,
                source_label_from_client_order_id(&o.client_order_id),
                now_ms,
            ],
        )?;
        source_by_order_id.insert(
            o.order_id,
            source_label_from_client_order_id(&o.client_order_id).to_string(),
        );
    }

    for t in trades {
        tx.execute(
            r#"
            INSERT INTO order_history_trades (
                symbol, trade_id, order_id, side, qty, price, commission, commission_asset, event_time_ms, realized_pnl, source, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(symbol, trade_id) DO UPDATE SET
                order_id = excluded.order_id,
                side = excluded.side,
                qty = excluded.qty,
                price = excluded.price,
                commission = excluded.commission,
                commission_asset = excluded.commission_asset,
                event_time_ms = excluded.event_time_ms,
                realized_pnl = excluded.realized_pnl,
                source = excluded.source,
                updated_at_ms = excluded.updated_at_ms
            "#,
            params![
                symbol,
                t.id as i64,
                t.order_id as i64,
                if t.is_buyer { "BUY" } else { "SELL" },
                t.qty,
                t.price,
                t.commission,
                t.commission_asset,
                t.time as i64,
                t.realized_pnl,
                source_by_order_id
                    .get(&t.order_id)
                    .map(String::as_str)
                    .unwrap_or("UNKNOWN"),
                now_ms,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn source_label_from_client_order_id(client_order_id: &str) -> &'static str {
    if client_order_id.contains("-mnl-") {
        "MANUAL"
    } else if client_order_id.contains("-cfg-") {
        "MA(Config)"
    } else if client_order_id.contains("-fst-") {
        "MA(Fast 5/20)"
    } else if client_order_id.contains("-slw-") {
        "MA(Slow 20/60)"
    } else {
        "UNKNOWN"
    }
}

pub fn load_persisted_trades(symbol: &str) -> Result<Vec<PersistedTrade>> {
    std::fs::create_dir_all("data")?;
    let conn = Connection::open("data/order_history.sqlite")?;
    ensure_trade_schema(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT trade_id, order_id, side, qty, price, commission, commission_asset, event_time_ms, realized_pnl, source
        FROM order_history_trades
        WHERE symbol = ?1
        ORDER BY event_time_ms ASC, trade_id ASC
        "#,
    )?;

    let rows = stmt.query_map([symbol], |row| {
        let side: String = row.get(2)?;
        let is_buyer = side.eq_ignore_ascii_case("BUY");
        let trade = BinanceMyTrade {
            symbol: symbol.to_string(),
            id: row.get::<_, i64>(0)? as u64,
            order_id: row.get::<_, i64>(1)? as u64,
            price: row.get(4)?,
            qty: row.get(3)?,
            commission: row.get(5)?,
            commission_asset: row.get(6)?,
            time: row.get::<_, i64>(7)? as u64,
            realized_pnl: row.get(8)?,
            is_buyer,
            is_maker: false,
        };
        Ok(PersistedTrade {
            trade,
            source: row.get(9)?,
        })
    })?;

    let mut trades = Vec::new();
    for row in rows {
        trades.push(row?);
    }
    Ok(trades)
}

pub fn load_last_trade_id(symbol: &str) -> Result<Option<u64>> {
    std::fs::create_dir_all("data")?;
    let conn = Connection::open("data/order_history.sqlite")?;
    let mut stmt = conn.prepare(
        r#"
        SELECT MAX(trade_id)
        FROM order_history_trades
        WHERE symbol = ?1
        "#,
    )?;
    let max_id = stmt.query_row([symbol], |row| row.get::<_, Option<i64>>(0))?;
    Ok(max_id.map(|v| v as u64))
}

pub fn load_trade_count(symbol: &str) -> Result<usize> {
    std::fs::create_dir_all("data")?;
    let conn = Connection::open("data/order_history.sqlite")?;
    let mut stmt = conn.prepare(
        r#"
        SELECT COUNT(*)
        FROM order_history_trades
        WHERE symbol = ?1
        "#,
    )?;
    let count = stmt.query_row([symbol], |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as usize)
}

#[derive(Clone, Copy, Default)]
struct LongPos {
    qty: f64,
    cost_quote: f64,
}

#[derive(Clone, Copy, Default)]
struct DailyBucket {
    pnl: f64,
    basis: f64,
}

pub fn load_realized_returns_by_bucket(
    bucket: HistoryBucket,
    limit: usize,
) -> Result<Vec<DailyRealizedReturn>> {
    std::fs::create_dir_all("data")?;
    let conn = Connection::open("data/order_history.sqlite")?;
    ensure_trade_schema(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT symbol, trade_id, order_id, side, qty, price, commission, commission_asset, event_time_ms, realized_pnl
        FROM order_history_trades
        ORDER BY symbol ASC, event_time_ms ASC, trade_id ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)? as u64,
            row.get::<_, i64>(2)? as u64,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)? as u64,
            row.get::<_, f64>(9)?,
        ))
    })?;

    let mut pos_by_symbol: HashMap<String, LongPos> = HashMap::new();
    let mut daily_by_key: HashMap<(String, String), DailyBucket> = HashMap::new();

    for row in rows {
        let (
            symbol,
            _trade_id,
            _order_id,
            side,
            qty_raw,
            price,
            commission,
            commission_asset,
            event_time_ms,
            realized_pnl,
        ) = row?;
        let qty = qty_raw.max(0.0);
        if qty <= f64::EPSILON {
            continue;
        }

        let (base_asset, quote_asset) = split_symbol_assets(&symbol);
        let fee_is_base =
            !base_asset.is_empty() && commission_asset.eq_ignore_ascii_case(&base_asset);
        let fee_is_quote =
            !quote_asset.is_empty() && commission_asset.eq_ignore_ascii_case(&quote_asset);
        let pos = pos_by_symbol.entry(symbol.clone()).or_default();

        let date = chrono::Utc
            .timestamp_millis_opt(event_time_ms as i64)
            .single()
            .map(|dt| dt.with_timezone(&chrono::Local))
            .map(|dt| match bucket {
                HistoryBucket::Day => dt.format("%Y-%m-%d").to_string(),
                HistoryBucket::Hour => dt.format("%Y-%m-%d %H:00").to_string(),
                HistoryBucket::Month => dt.format("%Y-%m").to_string(),
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Futures: realized_pnl is provided directly by exchange, do not apply spot long inventory logic.
        if symbol.ends_with("#FUT") {
            let basis = (qty * price).abs();
            let bucket = daily_by_key.entry((symbol.clone(), date)).or_default();
            bucket.pnl += realized_pnl;
            bucket.basis += basis;
            continue;
        }

        if side.eq_ignore_ascii_case("BUY") {
            let net_qty = (qty
                - if fee_is_base {
                    commission.max(0.0)
                } else {
                    0.0
                })
            .max(0.0);
            if net_qty <= f64::EPSILON {
                continue;
            }
            let fee_quote = if fee_is_quote {
                commission.max(0.0)
            } else {
                0.0
            };
            pos.qty += net_qty;
            pos.cost_quote += qty * price + fee_quote;
            continue;
        }

        if pos.qty <= f64::EPSILON {
            continue;
        }
        let close_qty = qty.min(pos.qty);
        if close_qty <= f64::EPSILON {
            continue;
        }
        let avg_cost = pos.cost_quote / pos.qty.max(f64::EPSILON);
        let fee_quote_total = if fee_is_quote {
            commission.max(0.0)
        } else if fee_is_base {
            commission.max(0.0) * price
        } else {
            0.0
        };
        let fee_quote = fee_quote_total * (close_qty / qty.max(f64::EPSILON));
        let realized_pnl = (close_qty * price - fee_quote) - (avg_cost * close_qty);
        let realized_basis = avg_cost * close_qty;

        let bucket = daily_by_key.entry((symbol.clone(), date)).or_default();
        bucket.pnl += realized_pnl;
        bucket.basis += realized_basis;

        pos.qty -= close_qty;
        pos.cost_quote -= realized_basis;
        if pos.qty <= f64::EPSILON {
            pos.qty = 0.0;
            pos.cost_quote = 0.0;
        }
    }

    let mut out: Vec<DailyRealizedReturn> = daily_by_key
        .into_iter()
        .map(|((symbol, date), b)| DailyRealizedReturn {
            symbol,
            date,
            realized_return_pct: if b.basis.abs() > f64::EPSILON {
                (b.pnl / b.basis) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.symbol.cmp(&b.symbol)));
    if out.len() > limit {
        out.truncate(limit);
    }
    Ok(out)
}

pub fn load_daily_realized_returns(limit: usize) -> Result<Vec<DailyRealizedReturn>> {
    load_realized_returns_by_bucket(HistoryBucket::Day, limit)
}

fn split_symbol_assets(symbol: &str) -> (String, String) {
    const QUOTE_SUFFIXES: [&str; 10] = [
        "USDT", "USDC", "FDUSD", "BUSD", "TUSD", "TRY", "EUR", "BTC", "ETH", "BNB",
    ];
    for q in QUOTE_SUFFIXES {
        if let Some(base) = symbol.strip_suffix(q) {
            if !base.is_empty() {
                return (base.to_string(), q.to_string());
            }
        }
    }
    (symbol.to_string(), String::new())
}
