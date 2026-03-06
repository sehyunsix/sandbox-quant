use crate::domain::exposure::Exposure;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetNotional {
    pub target_usdt: f64,
}

pub fn exposure_to_notional(exposure: Exposure, equity_usdt: f64) -> TargetNotional {
    TargetNotional {
        target_usdt: exposure.value() * equity_usdt,
    }
}
