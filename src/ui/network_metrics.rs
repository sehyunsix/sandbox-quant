#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkHealth {
    Ok,
    Warn,
    Crit,
}

pub fn count_since(events_ms: &[u64], now_ms: u64, window_ms: u64) -> usize {
    let lower = now_ms.saturating_sub(window_ms);
    events_ms.iter().filter(|&&ts| ts >= lower).count()
}

pub fn rate_per_sec(count: usize, window_sec: f64) -> f64 {
    if window_sec <= f64::EPSILON {
        return 0.0;
    }
    (count as f64) / window_sec
}

pub fn ratio_pct(numer: usize, denom: usize) -> f64 {
    if denom == 0 {
        return 0.0;
    }
    (numer as f64) * 100.0 / (denom as f64)
}

pub fn percentile(samples: &[u64], pct: usize) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let idx = ((sorted.len() * pct) / 100).min(sorted.len() - 1);
    sorted.get(idx).copied()
}

pub fn classify_health(
    ws_connected: bool,
    drop_ratio_10s_pct: f64,
    reconnect_rate_60s: f64,
    tick_latency_p95_ms: Option<u64>,
    heartbeat_gap_ms: Option<u64>,
) -> NetworkHealth {
    if !ws_connected {
        return NetworkHealth::Crit;
    }
    let mut severity = NetworkHealth::Ok;

    if drop_ratio_10s_pct >= 5.0 {
        return NetworkHealth::Crit;
    } else if drop_ratio_10s_pct >= 1.0 {
        severity = NetworkHealth::Warn;
    }

    if reconnect_rate_60s >= 5.0 {
        return NetworkHealth::Crit;
    } else if reconnect_rate_60s >= 2.0 {
        severity = NetworkHealth::Warn;
    }

    if let Some(p95) = tick_latency_p95_ms {
        if p95 >= 4_000 {
            return NetworkHealth::Crit;
        } else if p95 >= 1_500 {
            severity = NetworkHealth::Warn;
        }
    }

    if let Some(gap) = heartbeat_gap_ms {
        if gap >= 8_000 {
            return NetworkHealth::Crit;
        } else if gap >= 3_000 {
            severity = NetworkHealth::Warn;
        }
    }

    severity
}
