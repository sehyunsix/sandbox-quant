use anyhow::Result;
use rusqlite::{params, Connection};

use crate::binance::types::{BinanceAllOrder, BinanceMyTrade};

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
