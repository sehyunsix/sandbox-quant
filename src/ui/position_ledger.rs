use std::collections::{HashMap, VecDeque};

use crate::order_store::PersistedTrade;

#[derive(Debug, Clone)]
pub struct OpenOrderPosition {
    pub symbol: String,
    pub source_tag: String,
    pub order_id: u64,
    pub qty_open: f64,
    pub entry_price: f64,
    pub realized_pnl: f64,
}

fn canonical_source_tag(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return "sys".to_string();
    }
    if s.starts_with('c') && s[1..].chars().all(|ch| ch.is_ascii_digit()) {
        return s;
    }
    if s == "manual" {
        return "mnl".to_string();
    }
    if s.starts_with("ma(config") {
        return "cfg".to_string();
    }
    if s.starts_with("ma(") && s.contains("fast") {
        return "fst".to_string();
    }
    if s.starts_with("ma(") && s.contains("slow") {
        return "slw".to_string();
    }
    if let Some((head, _)) = s.split_once('(') {
        let h = head.trim();
        if !h.is_empty() {
            return h.to_string();
        }
    }
    s
}

#[derive(Debug, Clone)]
struct OpenLot {
    source_tag: String,
    order_id: u64,
    qty_open: f64,
    qty_total: f64,
    notional_total: f64,
    realized_pnl: f64,
}

impl OpenLot {
    fn entry_price(&self) -> f64 {
        if self.qty_total <= f64::EPSILON {
            0.0
        } else {
            self.notional_total / self.qty_total
        }
    }
}

pub fn build_open_order_positions_from_trades(trades: &[PersistedTrade]) -> Vec<OpenOrderPosition> {
    let mut sorted = trades.to_vec();
    sorted.sort_by_key(|t| (t.trade.symbol.clone(), t.trade.time, t.trade.id));

    let mut open_by_symbol: HashMap<String, VecDeque<OpenLot>> = HashMap::new();
    for row in sorted {
        let symbol = row.trade.symbol.trim().to_ascii_uppercase();
        let qty = row.trade.qty.max(0.0);
        if qty <= f64::EPSILON {
            continue;
        }

        let lots = open_by_symbol.entry(symbol.clone()).or_default();
        if row.trade.is_buyer {
            if let Some(last) = lots.back_mut() {
                if last.order_id == row.trade.order_id {
                    last.qty_open += qty;
                    last.qty_total += qty;
                    last.notional_total += qty * row.trade.price;
                    continue;
                }
            }
            lots.push_back(OpenLot {
                source_tag: canonical_source_tag(&row.source),
                order_id: row.trade.order_id,
                qty_open: qty,
                qty_total: qty,
                notional_total: qty * row.trade.price,
                realized_pnl: 0.0,
            });
            continue;
        }

        let mut remaining = qty;
        while remaining > f64::EPSILON {
            let Some(mut lot) = lots.pop_front() else {
                break;
            };
            let close_qty = remaining.min(lot.qty_open);
            let entry = lot.entry_price();
            lot.qty_open -= close_qty;
            lot.realized_pnl += (row.trade.price - entry) * close_qty;
            remaining -= close_qty;
            if lot.qty_open > f64::EPSILON {
                lots.push_front(lot);
                break;
            }
        }
    }

    let mut out = Vec::new();
    for (symbol, lots) in open_by_symbol {
        for lot in lots {
            if lot.qty_open <= f64::EPSILON {
                continue;
            }
            let entry_price = lot.entry_price();
            out.push(OpenOrderPosition {
                symbol: symbol.clone(),
                source_tag: lot.source_tag,
                order_id: lot.order_id,
                qty_open: lot.qty_open,
                entry_price,
                realized_pnl: lot.realized_pnl,
            });
        }
    }
    out.sort_by(|a, b| {
        a.symbol
            .cmp(&b.symbol)
            .then_with(|| a.source_tag.cmp(&b.source_tag))
            .then_with(|| a.order_id.cmp(&b.order_id))
    });
    out
}
