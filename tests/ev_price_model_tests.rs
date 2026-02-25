use sandbox_quant::ev::{
    futures_ev_from_y_normal, spot_ev_from_y_normal, FuturesEvInputs, PositionSide, SpotEvInputs,
    YNormal,
};

#[test]
fn spot_ev_is_higher_for_higher_mu() {
    let base = SpotEvInputs {
        p0: 100.0,
        qty: 1.0,
        side: PositionSide::Long,
        fee: 0.1,
        slippage: 0.1,
        borrow: 0.0,
    };
    let low = spot_ev_from_y_normal(
        YNormal {
            mu: -0.01,
            sigma: 0.02,
        },
        base,
    );
    let high = spot_ev_from_y_normal(
        YNormal {
            mu: 0.01,
            sigma: 0.02,
        },
        base,
    );
    assert!(high.ev > low.ev);
    assert!((0.0..=1.0).contains(&high.p_win));
}

#[test]
fn futures_multiplier_scales_ev_and_std() {
    let y = YNormal {
        mu: 0.002,
        sigma: 0.03,
    };
    let a = futures_ev_from_y_normal(
        y,
        FuturesEvInputs {
            p0: 50000.0,
            qty: 0.01,
            multiplier: 1.0,
            side: PositionSide::Long,
            fee: 0.1,
            slippage: 0.1,
            funding: 0.01,
            liq_risk: 0.0,
        },
    );
    let b = futures_ev_from_y_normal(
        y,
        FuturesEvInputs {
            p0: 50000.0,
            qty: 0.01,
            multiplier: 2.0,
            side: PositionSide::Long,
            fee: 0.1,
            slippage: 0.1,
            funding: 0.01,
            liq_risk: 0.0,
        },
    );
    assert!(b.ev > a.ev);
    assert!(b.ev_std > a.ev_std);
    assert!((0.0..=1.0).contains(&b.p_win));
}
