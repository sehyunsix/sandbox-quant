use crate::model::signal::Signal;
use crate::model::tick::Tick;
use crate::strategy::aroon_trend::AroonTrendStrategy;
use crate::strategy::atr_expansion::AtrExpansionStrategy;
use crate::strategy::bollinger_reversion::BollingerReversionStrategy;
use crate::strategy::channel_breakout::ChannelBreakoutStrategy;
use crate::strategy::donchian_trend::DonchianTrendStrategy;
use crate::strategy::ema_crossover::EmaCrossover;
use crate::strategy::ensemble_vote::EnsembleVoteStrategy;
use crate::strategy::ma_crossover::MaCrossover;
use crate::strategy::ma_reversion::MaReversionStrategy;
use crate::strategy::macd_crossover::MacdCrossoverStrategy;
use crate::strategy::opening_range_breakout::OpeningRangeBreakoutStrategy;
use crate::strategy::regime_switch::RegimeSwitchStrategy;
use crate::strategy::roc_momentum::RocMomentumStrategy;
use crate::strategy::rsa::RsaStrategy;
use crate::strategy::stochastic_reversion::StochasticReversionStrategy;
use crate::strategy::volatility_compression::VolatilityCompressionStrategy;
use crate::strategy_catalog::{StrategyKind, StrategyProfile};

#[derive(Debug)]
pub enum StrategyRuntime {
    Ma(MaCrossover),
    Ema(EmaCrossover),
    Atr(AtrExpansionStrategy),
    Vlc(VolatilityCompressionStrategy),
    Chb(ChannelBreakoutStrategy),
    Orb(OpeningRangeBreakoutStrategy),
    Rsa(RsaStrategy),
    Dct(DonchianTrendStrategy),
    Mrv(MaReversionStrategy),
    Bbr(BollingerReversionStrategy),
    Sto(StochasticReversionStrategy),
    Reg(RegimeSwitchStrategy),
    Ens(EnsembleVoteStrategy),
    Mac(MacdCrossoverStrategy),
    Roc(RocMomentumStrategy),
    Arn(AroonTrendStrategy),
}

impl StrategyRuntime {
    pub fn from_profile(profile: &StrategyProfile) -> Self {
        let (fast, slow, min_ticks) = profile.periods_tuple();
        match profile.strategy_kind() {
            StrategyKind::Rsa => {
                let period = fast.max(2);
                let upper = slow.clamp(51, 95) as f64;
                let lower = 100.0 - upper;
                Self::Rsa(RsaStrategy::new(period, lower, upper, min_ticks))
            }
            StrategyKind::Dct => Self::Dct(DonchianTrendStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Mrv => Self::Mrv(MaReversionStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Bbr => Self::Bbr(BollingerReversionStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Sto => Self::Sto(StochasticReversionStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Atr => Self::Atr(AtrExpansionStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Vlc => {
                Self::Vlc(VolatilityCompressionStrategy::new(fast, slow, min_ticks))
            }
            StrategyKind::Chb => Self::Chb(ChannelBreakoutStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Orb => {
                Self::Orb(OpeningRangeBreakoutStrategy::new(fast, slow, min_ticks))
            }
            StrategyKind::Ema => Self::Ema(EmaCrossover::new(fast, slow, min_ticks)),
            StrategyKind::Reg => Self::Reg(RegimeSwitchStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Ens => Self::Ens(EnsembleVoteStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Mac => Self::Mac(MacdCrossoverStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Roc => Self::Roc(RocMomentumStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Arn => Self::Arn(AroonTrendStrategy::new(fast, slow, min_ticks)),
            StrategyKind::Ma => Self::Ma(MaCrossover::new(fast, slow, min_ticks)),
        }
    }

    pub fn on_tick(&mut self, tick: &Tick) -> Signal {
        match self {
            Self::Ma(s) => s.on_tick(tick),
            Self::Ema(s) => s.on_tick(tick),
            Self::Atr(s) => s.on_tick(tick),
            Self::Vlc(s) => s.on_tick(tick),
            Self::Chb(s) => s.on_tick(tick),
            Self::Orb(s) => s.on_tick(tick),
            Self::Rsa(s) => s.on_tick(tick),
            Self::Dct(s) => s.on_tick(tick),
            Self::Mrv(s) => s.on_tick(tick),
            Self::Bbr(s) => s.on_tick(tick),
            Self::Sto(s) => s.on_tick(tick),
            Self::Reg(s) => s.on_tick(tick),
            Self::Ens(s) => s.on_tick(tick),
            Self::Mac(s) => s.on_tick(tick),
            Self::Roc(s) => s.on_tick(tick),
            Self::Arn(s) => s.on_tick(tick),
        }
    }

    pub fn fast_sma_value(&self) -> Option<f64> {
        match self {
            Self::Ma(s) => s.fast_sma_value(),
            Self::Ema(s) => s.fast_ema_value(),
            Self::Atr(_) => None,
            Self::Vlc(s) => s.mean_value(),
            Self::Chb(_) => None,
            Self::Orb(_) => None,
            Self::Rsa(_) => None,
            Self::Dct(_) => None,
            Self::Mrv(s) => s.mean_value(),
            Self::Bbr(s) => s.mean_value(),
            Self::Sto(_) => None,
            Self::Reg(_) => None,
            Self::Ens(_) => None,
            Self::Mac(_) => None,
            Self::Roc(_) => None,
            Self::Arn(_) => None,
        }
    }

    pub fn slow_sma_value(&self) -> Option<f64> {
        match self {
            Self::Ma(s) => s.slow_sma_value(),
            Self::Ema(s) => s.slow_ema_value(),
            Self::Atr(_) => None,
            Self::Vlc(_) => None,
            Self::Chb(_) => None,
            Self::Orb(_) => None,
            Self::Rsa(_) => None,
            Self::Dct(_) => None,
            Self::Mrv(_) => None,
            Self::Bbr(_) => None,
            Self::Sto(_) => None,
            Self::Reg(_) => None,
            Self::Ens(_) => None,
            Self::Mac(_) => None,
            Self::Roc(_) => None,
            Self::Arn(_) => None,
        }
    }
}
