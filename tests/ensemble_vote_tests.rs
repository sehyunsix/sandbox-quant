use sandbox_quant::model::signal::Signal;
use sandbox_quant::model::tick::Tick;
use sandbox_quant::strategy::ensemble_vote::EnsembleVoteStrategy;

fn tick(price: f64) -> Tick {
    Tick {
        symbol: "BTCUSDT".to_string(),
        price,
        qty: 1.0,
        timestamp_ms: 0,
        is_buyer_maker: false,
        trade_id: 0,
    }
}

#[test]
fn ensemble_vote_emits_actionable_signal() {
    let mut strategy = EnsembleVoteStrategy::new(5, 20, 0);

    // Deep pullback then rebound to force majority BUY votes.
    for p in [
        120.0, 118.0, 116.0, 114.0, 112.0, 110.0, 108.0, 106.0, 104.0, 102.0,
    ] {
        let _ = strategy.on_tick(&tick(p));
    }

    for p in [
        103.0, 105.0, 107.0, 109.0, 111.0, 113.0, 115.0, 116.0, 120.0, 118.0, 114.0, 110.0, 106.0,
        102.0, 99.0, 96.0, 93.0,
    ] {
        if strategy.on_tick(&tick(p)) != Signal::Hold {
            return;
        }
    }
    panic!("expected at least one actionable signal");
}

#[test]
fn ensemble_vote_respects_cooldown() {
    let mut strategy = EnsembleVoteStrategy::new(4, 12, 3);

    for p in [100.0, 98.0, 96.0, 94.0, 92.0, 90.0, 88.0, 86.0, 84.0] {
        let _ = strategy.on_tick(&tick(p));
    }

    let early = strategy.on_tick(&tick(120.0));
    assert_eq!(early, Signal::Hold);
}
