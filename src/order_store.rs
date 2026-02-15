use anyhow::Result;
use rusqlite::{params, Connection};

use crate::binance::types::{BinanceAllOrder, BinanceMyTrade};

#[derive(Debug, Clone)]
pub struct PersistedTrade {
    pub trade: BinanceMyTrade,
    pub source: String,
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
            event_time_ms INTEGER NOT NULL,
            source TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(symbol, trade_id)
        );
        "#,
    )?;

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
                symbol, trade_id, order_id, side, qty, price, event_time_ms, source, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(symbol, trade_id) DO UPDATE SET
                order_id = excluded.order_id,
                side = excluded.side,
                qty = excluded.qty,
                price = excluded.price,
                event_time_ms = excluded.event_time_ms,
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
                t.time as i64,
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
    let mut stmt = conn.prepare(
        r#"
        SELECT trade_id, order_id, side, qty, price, event_time_ms, source
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
            commission: 0.0,
            commission_asset: String::new(),
            time: row.get::<_, i64>(5)? as u64,
            is_buyer,
            is_maker: false,
        };
        Ok(PersistedTrade {
            trade,
            source: row.get(6)?,
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
