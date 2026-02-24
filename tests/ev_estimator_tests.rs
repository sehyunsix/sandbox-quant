use anyhow::Result;
use sandbox_quant::ev::{
    ConfidenceLevel, EvEstimator, EvEstimatorConfig, TradeStatsReader, TradeStatsSample,
    TradeStatsWindow,
};

#[derive(Clone)]
struct MockReader {
    local: TradeStatsWindow,
    global: TradeStatsWindow,
}

impl TradeStatsReader for MockReader {
    fn load_local_stats(
        &self,
        _source_tag: &str,
        _instrument: &str,
        _lookback: usize,
    ) -> Result<TradeStatsWindow> {
        Ok(self.local.clone())
    }

    fn load_global_stats(&self, _source_tag: &str, _lookback: usize) -> Result<TradeStatsWindow> {
        Ok(self.global.clone())
    }
}

#[test]
fn estimator_returns_probability_snapshot_and_ev() {
    let local = TradeStatsWindow {
        samples: vec![
            TradeStatsSample {
                age_days: 1.0,
                pnl_usdt: 12.0,
                holding_ms: 1_000,
            },
            TradeStatsSample {
                age_days: 2.0,
                pnl_usdt: -6.0,
                holding_ms: 2_000,
            },
            TradeStatsSample {
                age_days: 3.0,
                pnl_usdt: 8.0,
                holding_ms: 1_500,
            },
        ],
    };
    let global = TradeStatsWindow {
        samples: vec![
            TradeStatsSample {
                age_days: 1.0,
                pnl_usdt: 4.0,
                holding_ms: 1_000,
            },
            TradeStatsSample {
                age_days: 1.5,
                pnl_usdt: -2.0,
                holding_ms: 2_000,
            },
        ],
    };
    let reader = MockReader { local, global };
    let estimator = EvEstimator::new(EvEstimatorConfig::default(), reader, 200);

    let snapshot = estimator
        .estimate_entry_expectancy("cfg", "BTCUSDT", 1_700_000_000_000)
        .expect("estimation should succeed");

    assert!(snapshot.probability.p_win > 0.0 && snapshot.probability.p_win < 1.0);
    assert!(snapshot.probability.p_tail_loss > 0.0 && snapshot.probability.p_tail_loss < 1.0);
    assert!(snapshot.expected_holding_ms > 0);
    assert!(!snapshot.ev_model_version.is_empty());
}

#[test]
fn estimator_marks_low_confidence_for_sparse_window() {
    let local = TradeStatsWindow {
        samples: vec![TradeStatsSample {
            age_days: 1.0,
            pnl_usdt: 5.0,
            holding_ms: 1_000,
        }],
    };
    let global = TradeStatsWindow::default();
    let reader = MockReader { local, global };
    let estimator = EvEstimator::new(EvEstimatorConfig::default(), reader, 50);
    let snapshot = estimator
        .estimate_entry_expectancy("cfg", "ETHUSDT", 1_700_000_000_000)
        .expect("estimation should succeed");

    assert_eq!(snapshot.probability.confidence, ConfidenceLevel::Low);
}
