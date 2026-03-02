use std::collections::{HashMap, VecDeque};

use crate::ev::{EwmaYModel, EwmaYModelConfig, YNormal};
use crate::model::signal::Signal;
use crate::order_manager::MarketKind;

pub const PREDICTOR_METRIC_WINDOW: usize = 1200;
pub const PREDICTOR_WINDOW_MAX: usize = 7_200;
pub const PREDICTOR_R2_MIN_SAMPLES: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictorKind {
    Ewma,
    Ar1,
    Holt,
    Kalman,
    LinearRls,
    TsmomRls,
    MeanRevOu,
    VolScaledMom,
    VarRatioAdapt,
    MicroRevAr,
    SelfCalibMom,
    FeatureRls,
    CrossAssetMacroRls,
}

#[derive(Debug, Clone, Copy)]
pub struct PredictorConfig {
    pub kind: PredictorKind,
    pub alpha_mean: f64,
    pub alpha_var: f64,
    pub min_sigma: f64,
    pub phi_clip: f64,
    pub beta_trend: f64,
    pub process_var: f64,
    pub measure_var: f64,
}

pub type PredictorSpecs = Vec<(String, PredictorConfig)>;

pub fn default_predictor_specs(base: EwmaYModelConfig) -> PredictorSpecs {
    vec![
        (
            "ewma-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Ewma,
                alpha_mean: 0.18,
                alpha_var: 0.18,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "ewma-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Ewma,
                alpha_mean: base.alpha_mean,
                alpha_var: base.alpha_var,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "ewma-slow-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Ewma,
                alpha_mean: 0.03,
                alpha_var: 0.03,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.04,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "ar1-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Ar1,
                alpha_mean: base.alpha_mean,
                alpha_var: base.alpha_var,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "ar1-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Ar1,
                alpha_mean: 0.18,
                alpha_var: 0.18,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "holt-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Holt,
                alpha_mean: base.alpha_mean,
                alpha_var: base.alpha_var,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "holt-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Holt,
                alpha_mean: 0.20,
                alpha_var: 0.18,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.15,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "kalman-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::Kalman,
                alpha_mean: base.alpha_mean,
                alpha_var: base.alpha_var,
                min_sigma: base.min_sigma,
                phi_clip: 0.98,
                beta_trend: 0.08,
                process_var: 1e-6,
                measure_var: 1e-4,
            },
        ),
        (
            "lin-ind-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::LinearRls,
                alpha_mean: 0.20,
                alpha_var: 0.10,
                min_sigma: base.min_sigma,
                phi_clip: 0.995,
                beta_trend: 0.05,
                process_var: 1e-2,
                measure_var: 0.0,
            },
        ),
        (
            "tsmom-rls-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::TsmomRls,
                alpha_mean: 0.20,
                alpha_var: 0.10,
                min_sigma: base.min_sigma,
                phi_clip: 0.995,
                beta_trend: 0.05,
                process_var: 1e-2,
                measure_var: 0.0,
            },
        ),
        (
            "ou-revert-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::MeanRevOu,
                alpha_mean: 0.002,
                alpha_var: 0.05,
                min_sigma: base.min_sigma,
                phi_clip: 0.02,
                beta_trend: 3.0,
                process_var: 0.0,
                measure_var: 0.0,
            },
        ),
        (
            "ou-revert-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::MeanRevOu,
                alpha_mean: 0.008,
                alpha_var: 0.05,
                min_sigma: base.min_sigma,
                phi_clip: 0.035,
                beta_trend: 3.0,
                process_var: 0.0,
                measure_var: 0.0,
            },
        ),
        (
            "volmom-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::VolScaledMom,
                alpha_mean: 0.10,
                alpha_var: 0.05,
                min_sigma: base.min_sigma,
                phi_clip: 0.015,
                beta_trend: 3.0,
                process_var: 0.02,
                measure_var: 0.0,
            },
        ),
        (
            "volmom-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::VolScaledMom,
                alpha_mean: 0.20,
                alpha_var: 0.08,
                min_sigma: base.min_sigma,
                phi_clip: 0.025,
                beta_trend: 3.0,
                process_var: 0.04,
                measure_var: 0.0,
            },
        ),
        (
            "varratio-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::VarRatioAdapt,
                alpha_mean: 0.15,
                alpha_var: 0.02,
                min_sigma: base.min_sigma,
                phi_clip: 0.015,
                beta_trend: 1.0,
                process_var: 0.06,
                measure_var: 0.0,
            },
        ),
        (
            "varratio-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::VarRatioAdapt,
                alpha_mean: 0.25,
                alpha_var: 0.04,
                min_sigma: base.min_sigma,
                phi_clip: 0.025,
                beta_trend: 1.0,
                process_var: 0.10,
                measure_var: 0.0,
            },
        ),
        (
            "microrev-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::MicroRevAr,
                alpha_mean: 0.04,
                alpha_var: 0.04,
                min_sigma: base.min_sigma,
                phi_clip: 0.15,
                beta_trend: 0.0,
                process_var: 0.0,
                measure_var: 0.0,
            },
        ),
        (
            "microrev-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::MicroRevAr,
                alpha_mean: 0.10,
                alpha_var: 0.10,
                min_sigma: base.min_sigma,
                phi_clip: 0.15,
                beta_trend: 0.0,
                process_var: 0.0,
                measure_var: 0.0,
            },
        ),
        (
            "selfcalib-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::SelfCalibMom,
                alpha_mean: 0.12,
                alpha_var: 0.05,
                min_sigma: base.min_sigma,
                phi_clip: 0.0,
                beta_trend: 0.03,
                process_var: 0.03,
                measure_var: 0.0,
            },
        ),
        (
            "selfcalib-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::SelfCalibMom,
                alpha_mean: 0.20,
                alpha_var: 0.08,
                min_sigma: base.min_sigma,
                phi_clip: 0.0,
                beta_trend: 0.05,
                process_var: 0.05,
                measure_var: 0.0,
            },
        ),
        (
            "feat-rls-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::FeatureRls,
                alpha_mean: 0.12,
                alpha_var: 0.04,
                min_sigma: base.min_sigma,
                phi_clip: 0.998,
                beta_trend: 0.02,
                process_var: 0.05,
                measure_var: 3.0,
            },
        ),
        (
            "feat-rls-robust-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::FeatureRls,
                alpha_mean: 0.08,
                alpha_var: 0.03,
                min_sigma: base.min_sigma,
                phi_clip: 0.999,
                beta_trend: 0.015,
                process_var: 0.08,
                measure_var: 2.2,
            },
        ),
        (
            "feat-rls-fast-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::FeatureRls,
                alpha_mean: 0.20,
                alpha_var: 0.08,
                min_sigma: base.min_sigma,
                phi_clip: 0.996,
                beta_trend: 0.04,
                process_var: 0.05,
                measure_var: 3.0,
            },
        ),
        (
            "xasset-macro-rls-v1".to_string(),
            PredictorConfig {
                kind: PredictorKind::CrossAssetMacroRls,
                alpha_mean: 0.10,
                alpha_var: 0.08,
                min_sigma: base.min_sigma,
                phi_clip: 0.998,
                beta_trend: 0.02,
                process_var: 0.05,
                measure_var: 2.5,
            },
        ),
    ]
}

pub fn default_predictor_horizons() -> Vec<(String, u64)> {
    vec![
        ("1m".to_string(), 60_000),
        ("3m".to_string(), 180_000),
        ("5m".to_string(), 300_000),
    ]
}

pub fn build_predictor_models(
    specs: &[(String, PredictorConfig)],
) -> HashMap<String, PredictorModel> {
    let mut out = HashMap::new();
    for (id, cfg) in specs {
        let model = match cfg.kind {
            PredictorKind::Ewma => PredictorModel::Ewma(EwmaYModel::new(EwmaYModelConfig {
                alpha_mean: cfg.alpha_mean,
                alpha_var: cfg.alpha_var,
                min_sigma: cfg.min_sigma,
            })),
            PredictorKind::Ar1 => PredictorModel::Ar1(Ar1YModel::new(Ar1YModelConfig {
                alpha_mean: cfg.alpha_mean,
                alpha_var: cfg.alpha_var,
                min_sigma: cfg.min_sigma,
                phi_clip: cfg.phi_clip,
            })),
            PredictorKind::Holt => PredictorModel::Holt(HoltYModel::new(HoltYModelConfig {
                alpha_mean: cfg.alpha_mean,
                beta_trend: cfg.beta_trend,
                alpha_var: cfg.alpha_var,
                min_sigma: cfg.min_sigma,
            })),
            PredictorKind::Kalman => {
                PredictorModel::Kalman(KalmanYModel::new(KalmanYModelConfig {
                    process_var: cfg.process_var,
                    measure_var: cfg.measure_var,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::LinearRls => {
                PredictorModel::LinearRls(LinearRlsYModel::new(LinearRlsYModelConfig {
                    alpha_fast: cfg.alpha_mean,
                    alpha_slow: cfg.beta_trend,
                    alpha_vol: cfg.alpha_var,
                    forgetting: cfg.phi_clip,
                    ridge: cfg.process_var,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::TsmomRls => {
                PredictorModel::TsmomRls(TsmomRlsYModel::new(LinearRlsYModelConfig {
                    alpha_fast: cfg.alpha_mean,
                    alpha_slow: cfg.beta_trend,
                    alpha_vol: cfg.alpha_var,
                    forgetting: cfg.phi_clip,
                    ridge: cfg.process_var,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::MeanRevOu => {
                PredictorModel::MeanRevOu(MeanRevOuYModel::new(MeanRevOuYModelConfig {
                    alpha_level: cfg.alpha_mean,
                    alpha_var: cfg.alpha_var,
                    kappa: cfg.phi_clip,
                    z_clip: cfg.beta_trend,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::VolScaledMom => {
                PredictorModel::VolScaledMom(VolScaledMomYModel::new(VolScaledMomYModelConfig {
                    alpha_fast: cfg.alpha_mean,
                    alpha_slow: cfg.process_var,
                    alpha_vol: cfg.alpha_var,
                    kappa: cfg.phi_clip,
                    signal_clip: cfg.beta_trend,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::VarRatioAdapt => {
                PredictorModel::VarRatioAdapt(VarRatioAdaptYModel::new(VarRatioAdaptYModelConfig {
                    alpha_fast_var: cfg.alpha_mean,
                    alpha_slow_var: cfg.alpha_var,
                    alpha_trend: cfg.process_var,
                    kappa: cfg.phi_clip,
                    regime_clip: cfg.beta_trend,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::MicroRevAr => {
                PredictorModel::MicroRevAr(MicroRevArYModel::new(MicroRevArYModelConfig {
                    alpha: cfg.alpha_mean,
                    phi_max: cfg.phi_clip,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::SelfCalibMom => {
                PredictorModel::SelfCalibMom(SelfCalibMomYModel::new(SelfCalibMomYModelConfig {
                    alpha_fast: cfg.alpha_mean,
                    alpha_slow: cfg.beta_trend,
                    alpha_var: cfg.alpha_var,
                    alpha_calib: cfg.process_var,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::FeatureRls => {
                PredictorModel::FeatureRls(FeatureRlsYModel::new(FeatureRlsYModelConfig {
                    alpha_fast: cfg.alpha_mean,
                    alpha_slow: cfg.beta_trend,
                    alpha_var: cfg.alpha_var,
                    forgetting: cfg.phi_clip,
                    ridge: cfg.process_var,
                    pred_clip: cfg.measure_var,
                    min_sigma: cfg.min_sigma,
                }))
            }
            PredictorKind::CrossAssetMacroRls => PredictorModel::CrossAssetMacroRls(
                CrossAssetMacroRlsYModel::new(CrossAssetMacroRlsYModelConfig {
                    alpha_factor: cfg.alpha_mean,
                    alpha_resid: cfg.alpha_var,
                    forgetting: cfg.phi_clip,
                    ridge: cfg.process_var,
                    pred_clip: cfg.measure_var,
                    min_sigma: cfg.min_sigma,
                }),
            ),
        };
        out.insert(id.clone(), model);
    }
    out
}

#[derive(Debug)]
pub enum PredictorModel {
    Ewma(EwmaYModel),
    Ar1(Ar1YModel),
    Holt(HoltYModel),
    Kalman(KalmanYModel),
    LinearRls(LinearRlsYModel),
    TsmomRls(TsmomRlsYModel),
    MeanRevOu(MeanRevOuYModel),
    VolScaledMom(VolScaledMomYModel),
    VarRatioAdapt(VarRatioAdaptYModel),
    MicroRevAr(MicroRevArYModel),
    SelfCalibMom(SelfCalibMomYModel),
    FeatureRls(FeatureRlsYModel),
    CrossAssetMacroRls(CrossAssetMacroRlsYModel),
}

impl PredictorModel {
    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        match self {
            Self::Ewma(m) => m.observe_price(instrument, price),
            Self::Ar1(m) => m.observe_price(instrument, price),
            Self::Holt(m) => m.observe_price(instrument, price),
            Self::Kalman(m) => m.observe_price(instrument, price),
            Self::LinearRls(m) => m.observe_price(instrument, price),
            Self::TsmomRls(m) => m.observe_price(instrument, price),
            Self::MeanRevOu(m) => m.observe_price(instrument, price),
            Self::VolScaledMom(m) => m.observe_price(instrument, price),
            Self::VarRatioAdapt(m) => m.observe_price(instrument, price),
            Self::MicroRevAr(m) => m.observe_price(instrument, price),
            Self::SelfCalibMom(m) => m.observe_price(instrument, price),
            Self::FeatureRls(m) => m.observe_price(instrument, price),
            Self::CrossAssetMacroRls(m) => m.observe_price(instrument, price),
        }
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        match self {
            Self::Ewma(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::Ar1(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::Holt(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::Kalman(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::LinearRls(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::TsmomRls(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::MeanRevOu(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::VolScaledMom(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::VarRatioAdapt(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::MicroRevAr(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::SelfCalibMom(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::FeatureRls(m) => m.observe_signal_price(instrument, source_tag, signal, price),
            Self::CrossAssetMacroRls(m) => {
                m.observe_signal_price(instrument, source_tag, signal, price)
            }
        }
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        match self {
            Self::Ewma(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::Ar1(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::Holt(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::Kalman(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::LinearRls(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::TsmomRls(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::MeanRevOu(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::VolScaledMom(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::VarRatioAdapt(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::MicroRevAr(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::SelfCalibMom(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::FeatureRls(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
            Self::CrossAssetMacroRls(m) => m.estimate_base(instrument, fallback_mu, fallback_sigma),
        }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        match self {
            Self::Ewma(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::Ar1(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::Holt(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::Kalman(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::LinearRls(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::TsmomRls(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::MeanRevOu(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::VolScaledMom(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::VarRatioAdapt(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::MicroRevAr(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::SelfCalibMom(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::FeatureRls(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
            Self::CrossAssetMacroRls(m) => {
                m.estimate_for_signal(instrument, source_tag, signal, fallback_mu, fallback_sigma)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ar1YModelConfig {
    pub alpha_mean: f64,
    pub alpha_var: f64,
    pub min_sigma: f64,
    pub phi_clip: f64,
}

impl Default for Ar1YModelConfig {
    fn default() -> Self {
        Self {
            alpha_mean: 0.08,
            alpha_var: 0.08,
            min_sigma: 0.001,
            phi_clip: 0.98,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Ar1State {
    last_price: Option<f64>,
    last_return: Option<f64>,
    mu: f64,
    var: f64,
    cov1: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct Ar1YModel {
    cfg: Ar1YModelConfig,
    by_instrument: HashMap<String, Ar1State>,
    by_scope_side: HashMap<String, Ar1State>,
}

impl Ar1YModel {
    pub fn new(cfg: Ar1YModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = ar1_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) = ar1_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut Ar1State, price: f64, cfg: Ar1YModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let prev_mu = st.mu;
                let a_mu = cfg.alpha_mean.clamp(0.0, 1.0);
                let a_var = cfg.alpha_var.clamp(0.0, 1.0);
                st.mu = if st.samples == 0 {
                    r
                } else {
                    (1.0 - a_mu) * st.mu + a_mu * r
                };
                let centered = r - prev_mu;
                let sample_var = centered * centered;
                st.var = if st.samples == 0 {
                    sample_var
                } else {
                    (1.0 - a_var) * st.var + a_var * sample_var
                };
                if let Some(prev_r) = st.last_return {
                    let cov = (prev_r - prev_mu) * (r - prev_mu);
                    st.cov1 = if st.samples <= 1 {
                        cov
                    } else {
                        (1.0 - a_var) * st.cov1 + a_var * cov
                    };
                }
                st.last_return = Some(r);
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn ar1_forecast(
    st: &Ar1State,
    cfg: Ar1YModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    if st.samples == 0 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    let var = st.var.max(0.0);
    let mut phi = if var > 1e-12 { st.cov1 / var } else { 0.0 };
    phi = phi.clamp(-cfg.phi_clip, cfg.phi_clip);
    let mu = if let Some(last_r) = st.last_return {
        st.mu + phi * (last_r - st.mu)
    } else {
        st.mu
    };
    let eps_var = (1.0 - phi * phi).max(0.05) * var;
    let sigma = eps_var.sqrt().max(cfg.min_sigma);
    (mu, sigma)
}

#[derive(Debug, Clone, Copy)]
pub struct HoltYModelConfig {
    pub alpha_mean: f64,
    pub beta_trend: f64,
    pub alpha_var: f64,
    pub min_sigma: f64,
}

impl Default for HoltYModelConfig {
    fn default() -> Self {
        Self {
            alpha_mean: 0.08,
            beta_trend: 0.08,
            alpha_var: 0.08,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct HoltState {
    last_price: Option<f64>,
    level: f64,
    trend: f64,
    var: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct HoltYModel {
    cfg: HoltYModelConfig,
    by_instrument: HashMap<String, HoltState>,
    by_scope_side: HashMap<String, HoltState>,
}

impl HoltYModel {
    pub fn new(cfg: HoltYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let mu = if st.samples == 0 {
            fallback_mu
        } else {
            st.level + st.trend
        };
        let sigma = if st.samples == 0 {
            fallback_sigma.max(self.cfg.min_sigma)
        } else {
            st.var.sqrt().max(self.cfg.min_sigma)
        };
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_mu = scoped.level + scoped.trend;
        let scoped_sigma = scoped.var.sqrt().max(self.cfg.min_sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut HoltState, price: f64, cfg: HoltYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a = cfg.alpha_mean.clamp(0.0, 1.0);
                let b = cfg.beta_trend.clamp(0.0, 1.0);
                let a_var = cfg.alpha_var.clamp(0.0, 1.0);
                if st.samples == 0 {
                    st.level = r;
                    st.trend = 0.0;
                    st.var = 0.0;
                } else {
                    let pred = st.level + st.trend;
                    let new_level = a * r + (1.0 - a) * pred;
                    let new_trend = b * (new_level - st.level) + (1.0 - b) * st.trend;
                    let err = r - pred;
                    st.var = (1.0 - a_var) * st.var + a_var * (err * err);
                    st.level = new_level;
                    st.trend = new_trend;
                }
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KalmanYModelConfig {
    pub process_var: f64,
    pub measure_var: f64,
    pub min_sigma: f64,
}

impl Default for KalmanYModelConfig {
    fn default() -> Self {
        Self {
            process_var: 1e-6,
            measure_var: 1e-4,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct KalmanState {
    last_price: Option<f64>,
    x: f64,
    p: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct KalmanYModel {
    cfg: KalmanYModelConfig,
    by_instrument: HashMap<String, KalmanState>,
    by_scope_side: HashMap<String, KalmanState>,
}

impl KalmanYModel {
    pub fn new(cfg: KalmanYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        if st.samples == 0 {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        }
        let sigma = (st.p + self.cfg.measure_var.max(1e-12))
            .sqrt()
            .max(self.cfg.min_sigma);
        YNormal { mu: st.x, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_sigma = (scoped.p + self.cfg.measure_var.max(1e-12))
            .sqrt()
            .max(self.cfg.min_sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped.x + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut KalmanState, price: f64, cfg: KalmanYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let z = (price / prev).ln();
                if st.samples == 0 {
                    st.x = z;
                    st.p = cfg.measure_var.max(1e-12);
                    st.samples = 1;
                } else {
                    let q = cfg.process_var.max(1e-12);
                    let r = cfg.measure_var.max(1e-12);
                    let x_pred = st.x;
                    let p_pred = st.p + q;
                    let k = p_pred / (p_pred + r);
                    st.x = x_pred + k * (z - x_pred);
                    st.p = (1.0 - k) * p_pred;
                    st.samples = st.samples.saturating_add(1);
                }
            }
        }
        st.last_price = Some(price);
    }
}

const LINEAR_RLS_DIM: usize = 5;

#[derive(Debug, Clone, Copy)]
pub struct LinearRlsYModelConfig {
    pub alpha_fast: f64,
    pub alpha_slow: f64,
    pub alpha_vol: f64,
    pub forgetting: f64,
    pub ridge: f64,
    pub min_sigma: f64,
}

impl Default for LinearRlsYModelConfig {
    fn default() -> Self {
        Self {
            alpha_fast: 0.20,
            alpha_slow: 0.05,
            alpha_vol: 0.10,
            forgetting: 0.995,
            ridge: 1e-2,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LinearRlsState {
    last_price: Option<f64>,
    prev_r: f64,
    ema_fast: f64,
    ema_slow: f64,
    vol2: f64,
    resid2: f64,
    has_stats: bool,
    last_x: [f64; LINEAR_RLS_DIM],
    has_last_x: bool,
    beta: [f64; LINEAR_RLS_DIM],
    p: [[f64; LINEAR_RLS_DIM]; LINEAR_RLS_DIM],
    samples: u64,
}

impl Default for LinearRlsState {
    fn default() -> Self {
        Self {
            last_price: None,
            prev_r: 0.0,
            ema_fast: 0.0,
            ema_slow: 0.0,
            vol2: 0.0,
            resid2: 0.0,
            has_stats: false,
            last_x: [0.0; LINEAR_RLS_DIM],
            has_last_x: false,
            beta: [0.0; LINEAR_RLS_DIM],
            p: [[0.0; LINEAR_RLS_DIM]; LINEAR_RLS_DIM],
            samples: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct LinearRlsYModel {
    cfg: LinearRlsYModelConfig,
    by_instrument: HashMap<String, LinearRlsState>,
    by_scope_side: HashMap<String, LinearRlsState>,
}

impl LinearRlsYModel {
    pub fn new(cfg: LinearRlsYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        estimate_linear_state(st, self.cfg, fallback_mu, fallback_sigma)
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_est = estimate_linear_state(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_est.mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_est.sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut LinearRlsState, price: f64, cfg: LinearRlsYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                if !st.has_stats {
                    st.prev_r = r;
                    st.ema_fast = r;
                    st.ema_slow = r;
                    st.vol2 = r * r;
                    st.resid2 = r * r;
                    st.last_x = linear_features(st.prev_r, st.ema_fast, st.ema_slow, st.vol2);
                    st.has_last_x = true;
                    st.has_stats = true;
                    st.samples = 1;
                    init_rls_covariance(&mut st.p, cfg.ridge);
                } else {
                    let x = if st.has_last_x {
                        st.last_x
                    } else {
                        linear_features(st.prev_r, st.ema_fast, st.ema_slow, st.vol2)
                    };
                    let y_hat = dot(&st.beta, &x);
                    let err = r - y_hat;
                    rls_update(&mut st.beta, &mut st.p, &x, r, cfg.forgetting, cfg.ridge);

                    let a_vol = cfg.alpha_vol.clamp(0.0, 1.0);
                    st.resid2 = (1.0 - a_vol) * st.resid2 + a_vol * (err * err);
                    let a_fast = cfg.alpha_fast.clamp(0.0, 1.0);
                    let a_slow = cfg.alpha_slow.clamp(0.0, 1.0);
                    st.ema_fast = (1.0 - a_fast) * st.ema_fast + a_fast * r;
                    st.ema_slow = (1.0 - a_slow) * st.ema_slow + a_slow * r;
                    let centered = r - st.ema_slow;
                    st.vol2 = (1.0 - a_vol) * st.vol2 + a_vol * (centered * centered);
                    st.prev_r = r;
                    st.last_x = linear_features(st.prev_r, st.ema_fast, st.ema_slow, st.vol2);
                    st.has_last_x = true;
                    st.samples = st.samples.saturating_add(1);
                }
            }
        }
        st.last_price = Some(price);
    }
}

fn estimate_linear_state(
    st: &LinearRlsState,
    cfg: LinearRlsYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> YNormal {
    if !st.has_stats {
        return YNormal {
            mu: fallback_mu,
            sigma: fallback_sigma.max(cfg.min_sigma),
        };
    }
    let x = if st.has_last_x {
        st.last_x
    } else {
        linear_features(st.prev_r, st.ema_fast, st.ema_slow, st.vol2)
    };
    let pred = dot(&st.beta, &x);
    let n = st.samples as f64;
    let w = n / (n + 30.0);
    let mu = w * pred + (1.0 - w) * fallback_mu;
    let sigma_model = st.resid2.max(0.0).sqrt().max(cfg.min_sigma);
    let sigma =
        (w * sigma_model + (1.0 - w) * fallback_sigma.max(cfg.min_sigma)).max(cfg.min_sigma);
    YNormal { mu, sigma }
}

fn linear_features(prev_r: f64, ema_fast: f64, ema_slow: f64, vol2: f64) -> [f64; LINEAR_RLS_DIM] {
    [
        1.0,
        prev_r,
        ema_fast - ema_slow,
        vol2.max(0.0).sqrt(),
        prev_r.signum(),
    ]
}

fn dot(a: &[f64; LINEAR_RLS_DIM], b: &[f64; LINEAR_RLS_DIM]) -> f64 {
    let mut s = 0.0;
    for i in 0..LINEAR_RLS_DIM {
        s += a[i] * b[i];
    }
    s
}

fn init_rls_covariance(p: &mut [[f64; LINEAR_RLS_DIM]; LINEAR_RLS_DIM], ridge: f64) {
    let v = 1.0 / ridge.max(1e-9);
    for (i, row) in p.iter_mut().enumerate().take(LINEAR_RLS_DIM) {
        for (j, cell) in row.iter_mut().enumerate().take(LINEAR_RLS_DIM) {
            *cell = if i == j { v } else { 0.0 };
        }
    }
}

fn rls_update(
    beta: &mut [f64; LINEAR_RLS_DIM],
    p: &mut [[f64; LINEAR_RLS_DIM]; LINEAR_RLS_DIM],
    x: &[f64; LINEAR_RLS_DIM],
    y: f64,
    forgetting: f64,
    ridge: f64,
) {
    let lambda = forgetting.clamp(0.90, 0.9999);
    if p[0][0].abs() <= f64::EPSILON {
        init_rls_covariance(p, ridge);
    }
    let mut px = [0.0; LINEAR_RLS_DIM];
    for (i, px_i) in px.iter_mut().enumerate().take(LINEAR_RLS_DIM) {
        let mut v = 0.0;
        for (j, xj) in x.iter().enumerate().take(LINEAR_RLS_DIM) {
            v += p[i][j] * *xj;
        }
        *px_i = v;
    }
    let mut denom = lambda;
    for (i, x_i) in x.iter().enumerate().take(LINEAR_RLS_DIM) {
        denom += *x_i * px[i];
    }
    if !denom.is_finite() || denom.abs() <= 1e-12 {
        return;
    }
    let mut k = [0.0; LINEAR_RLS_DIM];
    for i in 0..LINEAR_RLS_DIM {
        k[i] = px[i] / denom;
    }
    let err = y - dot(beta, x);
    for i in 0..LINEAR_RLS_DIM {
        beta[i] += k[i] * err;
    }

    let mut x_t_p = [0.0; LINEAR_RLS_DIM];
    for (j, xtpj) in x_t_p.iter_mut().enumerate().take(LINEAR_RLS_DIM) {
        let mut v = 0.0;
        for (i, x_i) in x.iter().enumerate().take(LINEAR_RLS_DIM) {
            v += *x_i * p[i][j];
        }
        *xtpj = v;
    }
    let mut next_p = [[0.0; LINEAR_RLS_DIM]; LINEAR_RLS_DIM];
    for i in 0..LINEAR_RLS_DIM {
        for (j, xtpj) in x_t_p.iter().enumerate().take(LINEAR_RLS_DIM) {
            next_p[i][j] = (p[i][j] - k[i] * *xtpj) / lambda;
        }
    }
    *p = next_p;
}

const TSMOM_RET_BUF: usize = 24;

#[derive(Debug, Clone, Copy)]
struct TsmomRlsState {
    last_price: Option<f64>,
    returns: [f64; TSMOM_RET_BUF],
    ret_count: usize,
    ret_idx: usize,
    resid2: f64,
    has_stats: bool,
    last_x: [f64; LINEAR_RLS_DIM],
    has_last_x: bool,
    beta: [f64; LINEAR_RLS_DIM],
    p: [[f64; LINEAR_RLS_DIM]; LINEAR_RLS_DIM],
    samples: u64,
}

impl Default for TsmomRlsState {
    fn default() -> Self {
        Self {
            last_price: None,
            returns: [0.0; TSMOM_RET_BUF],
            ret_count: 0,
            ret_idx: 0,
            resid2: 0.0,
            has_stats: false,
            last_x: [0.0; LINEAR_RLS_DIM],
            has_last_x: false,
            beta: [0.0; LINEAR_RLS_DIM],
            p: [[0.0; LINEAR_RLS_DIM]; LINEAR_RLS_DIM],
            samples: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct TsmomRlsYModel {
    cfg: LinearRlsYModelConfig,
    by_instrument: HashMap<String, TsmomRlsState>,
    by_scope_side: HashMap<String, TsmomRlsState>,
}

impl TsmomRlsYModel {
    pub fn new(cfg: LinearRlsYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        estimate_tsmom_state(st, self.cfg, fallback_mu, fallback_sigma)
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_est = estimate_tsmom_state(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_est.mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_est.sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut TsmomRlsState, price: f64, cfg: LinearRlsYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                if st.has_last_x {
                    let y_hat = dot(&st.beta, &st.last_x);
                    let err = r - y_hat;
                    rls_update(
                        &mut st.beta,
                        &mut st.p,
                        &st.last_x,
                        r,
                        cfg.forgetting,
                        cfg.ridge,
                    );
                    let a = cfg.alpha_vol.clamp(0.0, 1.0);
                    st.resid2 = if st.has_stats {
                        (1.0 - a) * st.resid2 + a * (err * err)
                    } else {
                        err * err
                    };
                    st.has_stats = true;
                    st.samples = st.samples.saturating_add(1);
                } else {
                    init_rls_covariance(&mut st.p, cfg.ridge);
                }

                push_return(st, r);
                let x = tsmom_features(st);
                st.last_x = x;
                st.has_last_x = true;
                if st.samples == 0 {
                    st.samples = 1;
                }
            }
        }
        st.last_price = Some(price);
    }
}

fn push_return(st: &mut TsmomRlsState, r: f64) {
    st.returns[st.ret_idx] = r;
    st.ret_idx = (st.ret_idx + 1) % TSMOM_RET_BUF;
    st.ret_count = (st.ret_count + 1).min(TSMOM_RET_BUF);
}

fn recent_return(st: &TsmomRlsState, k_back: usize) -> f64 {
    if st.ret_count == 0 || k_back >= st.ret_count {
        return 0.0;
    }
    let pos = (st.ret_idx + TSMOM_RET_BUF - 1 - k_back) % TSMOM_RET_BUF;
    st.returns[pos]
}

fn mean_last(st: &TsmomRlsState, n: usize) -> f64 {
    let m = n.min(st.ret_count);
    if m == 0 {
        return 0.0;
    }
    let mut s = 0.0;
    for k in 0..m {
        s += recent_return(st, k);
    }
    s / m as f64
}

fn tsmom_features(st: &TsmomRlsState) -> [f64; LINEAR_RLS_DIM] {
    let m1 = mean_last(st, 1);
    let m3 = mean_last(st, 3);
    let m6 = mean_last(st, 6);
    let m12 = mean_last(st, 12);
    [1.0, m1, m3, m6, m12]
}

fn estimate_tsmom_state(
    st: &TsmomRlsState,
    cfg: LinearRlsYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> YNormal {
    if !st.has_last_x {
        return YNormal {
            mu: fallback_mu,
            sigma: fallback_sigma.max(cfg.min_sigma),
        };
    }
    let pred = dot(&st.beta, &st.last_x);
    let n = st.samples as f64;
    let w = n / (n + 30.0);
    let mu = w * pred + (1.0 - w) * fallback_mu;
    let sigma_model = st.resid2.max(0.0).sqrt().max(cfg.min_sigma);
    let sigma =
        (w * sigma_model + (1.0 - w) * fallback_sigma.max(cfg.min_sigma)).max(cfg.min_sigma);
    YNormal { mu, sigma }
}

#[derive(Debug, Clone, Copy)]
pub struct MeanRevOuYModelConfig {
    pub alpha_level: f64,
    pub alpha_var: f64,
    pub kappa: f64,
    pub z_clip: f64,
    pub min_sigma: f64,
}

impl Default for MeanRevOuYModelConfig {
    fn default() -> Self {
        Self {
            alpha_level: 0.002,
            alpha_var: 0.05,
            kappa: 0.02,
            z_clip: 3.0,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MeanRevOuState {
    last_price: Option<f64>,
    log_ema: f64,
    var: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct MeanRevOuYModel {
    cfg: MeanRevOuYModelConfig,
    by_instrument: HashMap<String, MeanRevOuState>,
    by_scope_side: HashMap<String, MeanRevOuState>,
}

impl MeanRevOuYModel {
    pub fn new(cfg: MeanRevOuYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = mean_revert_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) = mean_revert_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut MeanRevOuState, price: f64, cfg: MeanRevOuYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_level = cfg.alpha_level.clamp(0.0, 1.0);
                let a_var = cfg.alpha_var.clamp(0.0, 1.0);
                if st.samples == 0 {
                    st.log_ema = price.ln();
                    st.var = r * r;
                } else {
                    st.log_ema = (1.0 - a_level) * st.log_ema + a_level * price.ln();
                    st.var = (1.0 - a_var) * st.var + a_var * (r * r);
                }
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn mean_revert_forecast(
    st: &MeanRevOuState,
    cfg: MeanRevOuYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    if st.samples == 0 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    let current_log_price = match st.last_price {
        Some(p) if p > f64::EPSILON => p.ln(),
        _ => return (fallback_mu, fallback_sigma.max(cfg.min_sigma)),
    };
    let displacement = current_log_price - st.log_ema;
    let sigma = st.var.max(0.0).sqrt().max(cfg.min_sigma);
    let z_score = if sigma > 1e-9 {
        (displacement / sigma).clamp(-cfg.z_clip, cfg.z_clip)
    } else {
        0.0
    };
    let mu_raw = -cfg.kappa * z_score * sigma;
    let n = st.samples as f64;
    let w = n / (n + 100.0);
    let mu = w * mu_raw + (1.0 - w) * fallback_mu;
    (mu, sigma)
}

// ---------------------------------------------------------------------------
// VolScaledMom: Volatility-Normalized Time-Series Momentum
// Based on Moskowitz, Ooi, Pedersen (2012) "Time Series Momentum"
// Signal = (ema_fast - ema_slow) / vol   (Sharpe-ratio-like)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct VolScaledMomYModelConfig {
    pub alpha_fast: f64,
    pub alpha_slow: f64,
    pub alpha_vol: f64,
    pub kappa: f64,
    pub signal_clip: f64,
    pub min_sigma: f64,
}

impl Default for VolScaledMomYModelConfig {
    fn default() -> Self {
        Self {
            alpha_fast: 0.10,
            alpha_slow: 0.02,
            alpha_vol: 0.05,
            kappa: 0.015,
            signal_clip: 3.0,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct VolScaledMomState {
    last_price: Option<f64>,
    ema_fast: f64,
    ema_slow: f64,
    ema_vol: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct VolScaledMomYModel {
    cfg: VolScaledMomYModelConfig,
    by_instrument: HashMap<String, VolScaledMomState>,
    by_scope_side: HashMap<String, VolScaledMomState>,
}

impl VolScaledMomYModel {
    pub fn new(cfg: VolScaledMomYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = vol_scaled_mom_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) =
            vol_scaled_mom_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut VolScaledMomState, price: f64, cfg: VolScaledMomYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_f = cfg.alpha_fast.clamp(0.0, 1.0);
                let a_s = cfg.alpha_slow.clamp(0.0, 1.0);
                let a_v = cfg.alpha_vol.clamp(0.0, 1.0);
                if st.samples == 0 {
                    st.ema_fast = r;
                    st.ema_slow = r;
                    st.ema_vol = r.abs();
                } else {
                    st.ema_fast = (1.0 - a_f) * st.ema_fast + a_f * r;
                    st.ema_slow = (1.0 - a_s) * st.ema_slow + a_s * r;
                    st.ema_vol = (1.0 - a_v) * st.ema_vol + a_v * r.abs();
                }
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn vol_scaled_mom_forecast(
    st: &VolScaledMomState,
    cfg: VolScaledMomYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    if st.samples < 2 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    let vol = st.ema_vol.max(cfg.min_sigma);
    let momentum = st.ema_fast - st.ema_slow;
    let signal = (momentum / vol).clamp(-cfg.signal_clip, cfg.signal_clip);
    let mu_raw = cfg.kappa * signal * vol;
    let n = st.samples as f64;
    let w = n / (n + 100.0);
    let mu = w * mu_raw + (1.0 - w) * fallback_mu;
    // sigma from vol (ema of |r| ≈ sqrt(2/π) * sigma for normal)
    let sigma = (vol * 1.25).max(cfg.min_sigma); // approx sqrt(π/2) correction
    (mu, sigma)
}

// ---------------------------------------------------------------------------
// VarRatioAdapt: Variance-Ratio Regime-Adaptive Predictor
// Based on Lo & MacKinlay (1988) "Stock Market Prices Do Not Follow Random Walks"
// VR = fast_var / slow_var; VR>1 = trending, VR<1 = mean-reverting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct VarRatioAdaptYModelConfig {
    pub alpha_fast_var: f64,
    pub alpha_slow_var: f64,
    pub alpha_trend: f64,
    pub kappa: f64,
    pub regime_clip: f64,
    pub min_sigma: f64,
}

impl Default for VarRatioAdaptYModelConfig {
    fn default() -> Self {
        Self {
            alpha_fast_var: 0.15,
            alpha_slow_var: 0.02,
            alpha_trend: 0.06,
            kappa: 0.015,
            regime_clip: 1.0,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct VarRatioAdaptState {
    last_price: Option<f64>,
    var_fast: f64,
    var_slow: f64,
    ema_trend: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct VarRatioAdaptYModel {
    cfg: VarRatioAdaptYModelConfig,
    by_instrument: HashMap<String, VarRatioAdaptState>,
    by_scope_side: HashMap<String, VarRatioAdaptState>,
}

impl VarRatioAdaptYModel {
    pub fn new(cfg: VarRatioAdaptYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = var_ratio_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) = var_ratio_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut VarRatioAdaptState, price: f64, cfg: VarRatioAdaptYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let r2 = r * r;
                let a_f = cfg.alpha_fast_var.clamp(0.0, 1.0);
                let a_s = cfg.alpha_slow_var.clamp(0.0, 1.0);
                let a_t = cfg.alpha_trend.clamp(0.0, 1.0);
                if st.samples == 0 {
                    st.var_fast = r2;
                    st.var_slow = r2;
                    st.ema_trend = r;
                } else {
                    st.var_fast = (1.0 - a_f) * st.var_fast + a_f * r2;
                    st.var_slow = (1.0 - a_s) * st.var_slow + a_s * r2;
                    st.ema_trend = (1.0 - a_t) * st.ema_trend + a_t * r;
                }
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn var_ratio_forecast(
    st: &VarRatioAdaptState,
    cfg: VarRatioAdaptYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    if st.samples < 2 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    let sigma_slow = st.var_slow.max(0.0).sqrt().max(cfg.min_sigma);
    if st.var_slow < 1e-18 {
        return (fallback_mu, sigma_slow);
    }
    // Variance ratio: >1 = trending (positive autocorrelation), <1 = reverting
    let vr = st.var_fast / st.var_slow;
    // Regime strength: how far from random walk (VR=1)
    let regime = (vr - 1.0).clamp(-cfg.regime_clip, cfg.regime_clip);
    // Direction from recent trend EMA
    let direction = st.ema_trend.signum();
    // Prediction: regime * direction * kappa * sigma
    // Trending (regime>0) + upward direction → predict positive (continuation)
    // Reverting (regime<0) + upward direction → predict negative (reversion)
    let mu_raw = cfg.kappa * regime * direction * sigma_slow;
    let n = st.samples as f64;
    let w = n / (n + 100.0);
    let mu = w * mu_raw + (1.0 - w) * fallback_mu;
    (mu, sigma_slow)
}

// ---------------------------------------------------------------------------
// MicroRevAr: Microstructure Reversal AR(1)
// Based on Roll (1984) bid-ask bounce model.
// Only predicts when lag-1 autocovariance is negative (bounce detected).
// Predicts zero otherwise — avoids adding noise when no signal exists.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct MicroRevArYModelConfig {
    pub alpha: f64,
    pub phi_max: f64,
    pub min_sigma: f64,
}

impl Default for MicroRevArYModelConfig {
    fn default() -> Self {
        Self {
            alpha: 0.04,
            phi_max: 0.15,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MicroRevArState {
    last_price: Option<f64>,
    prev_return: f64,
    gamma1: f64,
    var_r: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct MicroRevArYModel {
    cfg: MicroRevArYModelConfig,
    by_instrument: HashMap<String, MicroRevArState>,
    by_scope_side: HashMap<String, MicroRevArState>,
}

impl MicroRevArYModel {
    pub fn new(cfg: MicroRevArYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = micro_rev_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) = micro_rev_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut MicroRevArState, price: f64, cfg: MicroRevArYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a = cfg.alpha.clamp(0.0, 1.0);
                if st.samples == 0 {
                    st.gamma1 = 0.0;
                    st.var_r = r * r;
                } else {
                    // gamma1 = EWMA of r_t * r_{t-1} (lag-1 autocovariance)
                    st.gamma1 = (1.0 - a) * st.gamma1 + a * (r * st.prev_return);
                    st.var_r = (1.0 - a) * st.var_r + a * (r * r);
                }
                st.prev_return = r;
                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn micro_rev_forecast(
    st: &MicroRevArState,
    cfg: MicroRevArYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    let sigma = st.var_r.max(0.0).sqrt().max(cfg.min_sigma);
    if st.samples < 3 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    // Only predict when autocovariance is negative (bid-ask bounce / reversal)
    // When gamma1 >= 0, there is momentum, not reversal — predict zero
    if st.gamma1 >= 0.0 || st.var_r < 1e-18 {
        return (0.0, sigma);
    }
    // phi = gamma1 / var_r, always negative here
    let phi = (st.gamma1 / st.var_r).clamp(-cfg.phi_max, 0.0);
    let mu_raw = phi * st.prev_return;
    // Very heavy shrinkage: n/(n+200)
    let n = st.samples as f64;
    let w = n / (n + 200.0);
    let mu = w * mu_raw;
    (mu, sigma)
}

// ---------------------------------------------------------------------------
// SelfCalibMom: Self-Calibrating Momentum
// Uses online feedback to learn the optimal prediction magnitude.
// Tracks cross-moment (Cov(y,ŷ)) and pred-variance (Var(ŷ)) to compute
// the optimal shrinkage: alpha* = Cov(y,ŷ)/Var(ŷ), clamped to [0,1].
// This is mathematically guaranteed to converge to R² = ρ² ≥ 0.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct SelfCalibMomYModelConfig {
    pub alpha_fast: f64,
    pub alpha_slow: f64,
    pub alpha_var: f64,
    pub alpha_calib: f64,
    pub min_sigma: f64,
}

impl Default for SelfCalibMomYModelConfig {
    fn default() -> Self {
        Self {
            alpha_fast: 0.12,
            alpha_slow: 0.03,
            alpha_var: 0.05,
            alpha_calib: 0.03,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SelfCalibMomState {
    last_price: Option<f64>,
    ema_fast: f64,
    ema_slow: f64,
    var_r: f64,
    prev_raw_mu: f64,
    cross: f64,
    pred_sq: f64,
    samples: u64,
}

#[derive(Debug, Default)]
pub struct SelfCalibMomYModel {
    cfg: SelfCalibMomYModelConfig,
    by_instrument: HashMap<String, SelfCalibMomState>,
    by_scope_side: HashMap<String, SelfCalibMomState>,
}

impl SelfCalibMomYModel {
    pub fn new(cfg: SelfCalibMomYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        let (mu, sigma) = self_calib_forecast(st, self.cfg, fallback_mu, fallback_sigma);
        YNormal { mu, sigma }
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let (scoped_mu, scoped_sigma) = self_calib_forecast(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut SelfCalibMomState, price: f64, cfg: SelfCalibMomYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_f = cfg.alpha_fast.clamp(0.0, 1.0);
                let a_s = cfg.alpha_slow.clamp(0.0, 1.0);
                let a_v = cfg.alpha_var.clamp(0.0, 1.0);
                let a_c = cfg.alpha_calib.clamp(0.0, 1.0);

                // Calibration feedback: actual return vs previous raw prediction
                if st.samples > 0 {
                    st.cross = (1.0 - a_c) * st.cross + a_c * (r * st.prev_raw_mu);
                    st.pred_sq = (1.0 - a_c) * st.pred_sq + a_c * (st.prev_raw_mu * st.prev_raw_mu);
                }

                // Update signal components
                if st.samples == 0 {
                    st.ema_fast = r;
                    st.ema_slow = r;
                    st.var_r = r * r;
                } else {
                    st.ema_fast = (1.0 - a_f) * st.ema_fast + a_f * r;
                    st.ema_slow = (1.0 - a_s) * st.ema_slow + a_s * r;
                    st.var_r = (1.0 - a_v) * st.var_r + a_v * (r * r);
                }

                // Compute raw prediction for next step's calibration
                let vol = st.var_r.max(0.0).sqrt().max(cfg.min_sigma);
                let momentum = st.ema_fast - st.ema_slow;
                let signal = if vol > 1e-12 { momentum / vol } else { 0.0 };
                // Base raw prediction: signal * vol * 0.1 (deliberate 10x shrinkage)
                st.prev_raw_mu = signal * vol * 0.1;

                st.samples = st.samples.saturating_add(1);
            }
        }
        st.last_price = Some(price);
    }
}

fn self_calib_forecast(
    st: &SelfCalibMomState,
    cfg: SelfCalibMomYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> (f64, f64) {
    let sigma = st.var_r.max(0.0).sqrt().max(cfg.min_sigma);
    if st.samples < 5 {
        return (fallback_mu, fallback_sigma.max(cfg.min_sigma));
    }
    // Self-calibrated shrinkage: alpha* = Cov(y, ŷ_raw) / Var(ŷ_raw)
    // Clamped to [0, 1]: negative means anti-correlated → predict zero
    let alpha_opt = if st.pred_sq > 1e-18 {
        (st.cross / st.pred_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mu_calibrated = alpha_opt * st.prev_raw_mu;
    // Additional sample-count shrinkage
    let n = st.samples as f64;
    let w = n / (n + 200.0);
    let mu = w * mu_calibrated + (1.0 - w) * fallback_mu;
    (mu, sigma)
}

// ---------------------------------------------------------------------------
// FeatureRls: Feature-Rich RLS with Novel Microstructure Features
//
// Features (7-dim):
//   0: intercept (1.0)
//   1: return acceleration  (r_t - r_{t-1}) / σ   — momentum curvature
//   2: realized skewness    EWMA(r³) / σ³          — return asymmetry
//   3: run-length signal    tanh(run_count/3) * sign — exhaustion
//   4: vol acceleration     (var_fast - var_slow) / var_slow — regime shift
//   5: normalized autocov   γ₁ / σ²                — reversal/momentum regime
//   6: return extremity     |r_prev| / σ            — extreme move detection
//
// Key difference from LinearRls: entirely different features + heavier
// regularization (ridge=0.05, forgetting=0.998) + prediction clipping.
// ---------------------------------------------------------------------------

const FEAT_RLS_DIM: usize = 7;

#[derive(Debug, Clone, Copy)]
pub struct FeatureRlsYModelConfig {
    pub alpha_fast: f64,
    pub alpha_slow: f64,
    pub alpha_var: f64,
    pub forgetting: f64,
    pub ridge: f64,
    pub pred_clip: f64,
    pub min_sigma: f64,
}

impl Default for FeatureRlsYModelConfig {
    fn default() -> Self {
        Self {
            alpha_fast: 0.12,
            alpha_slow: 0.02,
            alpha_var: 0.04,
            forgetting: 0.998,
            ridge: 0.05,
            pred_clip: 3.0,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FeatureRlsState {
    last_price: Option<f64>,
    prev_r: f64,
    prev_prev_r: f64,
    ema_r3: f64,
    var_fast: f64,
    var_slow: f64,
    gamma1: f64,
    run_count: i32,
    resid2: f64,
    has_stats: bool,
    last_x: [f64; FEAT_RLS_DIM],
    has_last_x: bool,
    beta: [f64; FEAT_RLS_DIM],
    p: [[f64; FEAT_RLS_DIM]; FEAT_RLS_DIM],
    samples: u64,
}

impl Default for FeatureRlsState {
    fn default() -> Self {
        Self {
            last_price: None,
            prev_r: 0.0,
            prev_prev_r: 0.0,
            ema_r3: 0.0,
            var_fast: 0.0,
            var_slow: 0.0,
            gamma1: 0.0,
            run_count: 0,
            resid2: 0.0,
            has_stats: false,
            last_x: [0.0; FEAT_RLS_DIM],
            has_last_x: false,
            beta: [0.0; FEAT_RLS_DIM],
            p: [[0.0; FEAT_RLS_DIM]; FEAT_RLS_DIM],
            samples: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct FeatureRlsYModel {
    cfg: FeatureRlsYModelConfig,
    by_instrument: HashMap<String, FeatureRlsState>,
    by_scope_side: HashMap<String, FeatureRlsState>,
}

impl FeatureRlsYModel {
    pub fn new(cfg: FeatureRlsYModelConfig) -> Self {
        Self {
            cfg,
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        Self::update_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_state(self.by_scope_side.entry(key).or_default(), price, self.cfg);
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        feat_rls_estimate(st, self.cfg, fallback_mu, fallback_sigma)
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_est = feat_rls_estimate(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_est.mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_est.sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn update_state(st: &mut FeatureRlsState, price: f64, cfg: FeatureRlsYModelConfig) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a_f = cfg.alpha_fast.clamp(0.0, 1.0);
                let a_s = cfg.alpha_slow.clamp(0.0, 1.0);
                let a_v = cfg.alpha_var.clamp(0.0, 1.0);

                if !st.has_stats {
                    st.prev_r = r;
                    st.prev_prev_r = 0.0;
                    st.ema_r3 = r * r * r;
                    st.var_fast = r * r;
                    st.var_slow = r * r;
                    st.gamma1 = 0.0;
                    st.run_count = if r >= 0.0 { 1 } else { -1 };
                    st.resid2 = r * r;
                    st.has_stats = true;
                    st.samples = 1;
                    feat_rls_init_p(&mut st.p, cfg.ridge);
                    st.last_x = feat_rls_features(st, cfg.min_sigma);
                    st.has_last_x = true;
                } else {
                    // RLS update with previous features
                    let x = if st.has_last_x {
                        st.last_x
                    } else {
                        feat_rls_features(st, cfg.min_sigma)
                    };
                    let y_hat = feat_rls_dot(&st.beta, &x);
                    let err = r - y_hat;
                    feat_rls_update(&mut st.beta, &mut st.p, &x, r, cfg.forgetting, cfg.ridge);
                    st.resid2 = (1.0 - a_v) * st.resid2 + a_v * (err * err);

                    // Update feature state
                    st.ema_r3 = (1.0 - a_v) * st.ema_r3 + a_v * (r * r * r);
                    st.var_fast = (1.0 - a_f) * st.var_fast + a_f * (r * r);
                    st.var_slow = (1.0 - a_s) * st.var_slow + a_s * (r * r);
                    st.gamma1 = (1.0 - a_v) * st.gamma1 + a_v * (r * st.prev_r);

                    // Run length tracking
                    if r >= 0.0 {
                        st.run_count = if st.run_count > 0 {
                            st.run_count.saturating_add(1)
                        } else {
                            1
                        };
                    } else {
                        st.run_count = if st.run_count < 0 {
                            st.run_count.saturating_sub(1)
                        } else {
                            -1
                        };
                    }

                    st.prev_prev_r = st.prev_r;
                    st.prev_r = r;
                    st.last_x = feat_rls_features(st, cfg.min_sigma);
                    st.has_last_x = true;
                    st.samples = st.samples.saturating_add(1);
                }
            }
        }
        st.last_price = Some(price);
    }
}

fn feat_rls_features(st: &FeatureRlsState, min_sigma: f64) -> [f64; FEAT_RLS_DIM] {
    let sigma = st.var_slow.max(0.0).sqrt().max(min_sigma);
    let sigma2 = st.var_slow.max(1e-18);
    let sigma3 = sigma * sigma2;

    // Feature 1: return acceleration (2nd derivative), normalized
    let accel = if sigma > 1e-12 {
        ((st.prev_r - st.prev_prev_r) / sigma).clamp(-5.0, 5.0)
    } else {
        0.0
    };

    // Feature 2: realized skewness proxy
    let skew = if sigma3 > 1e-18 {
        (st.ema_r3 / sigma3).clamp(-5.0, 5.0)
    } else {
        0.0
    };

    // Feature 3: run-length signal (tanh-normalized)
    let run_norm = (st.run_count as f64 / 3.0).tanh();

    // Feature 4: volatility acceleration
    let vol_accel = if st.var_slow > 1e-18 {
        ((st.var_fast - st.var_slow) / st.var_slow).clamp(-3.0, 3.0)
    } else {
        0.0
    };

    // Feature 5: normalized autocovariance (regime indicator)
    let autocov = if sigma2 > 1e-18 {
        (st.gamma1 / sigma2).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    // Feature 6: return extremity (|prev_r| / sigma)
    let extremity = if sigma > 1e-12 {
        (st.prev_r.abs() / sigma).clamp(0.0, 5.0)
    } else {
        0.0
    };

    [1.0, accel, skew, run_norm, vol_accel, autocov, extremity]
}

fn feat_rls_estimate(
    st: &FeatureRlsState,
    cfg: FeatureRlsYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> YNormal {
    if !st.has_stats || st.samples < 3 {
        return YNormal {
            mu: fallback_mu,
            sigma: fallback_sigma.max(cfg.min_sigma),
        };
    }
    let x = if st.has_last_x {
        st.last_x
    } else {
        feat_rls_features(st, cfg.min_sigma)
    };
    let pred = feat_rls_dot(&st.beta, &x);
    let sigma_model = st.resid2.max(0.0).sqrt().max(cfg.min_sigma);
    // Clip prediction to ±pred_clip * sigma (prevent extreme outputs)
    let pred_clipped = pred.clamp(-cfg.pred_clip * sigma_model, cfg.pred_clip * sigma_model);
    // Heavy sample-count shrinkage: n/(n+150)
    let n = st.samples as f64;
    let w = n / (n + 150.0);
    let mu = w * pred_clipped + (1.0 - w) * fallback_mu;
    let sigma =
        (w * sigma_model + (1.0 - w) * fallback_sigma.max(cfg.min_sigma)).max(cfg.min_sigma);
    YNormal { mu, sigma }
}

fn feat_rls_dot(a: &[f64; FEAT_RLS_DIM], b: &[f64; FEAT_RLS_DIM]) -> f64 {
    let mut s = 0.0;
    for i in 0..FEAT_RLS_DIM {
        s += a[i] * b[i];
    }
    s
}

fn feat_rls_init_p(p: &mut [[f64; FEAT_RLS_DIM]; FEAT_RLS_DIM], ridge: f64) {
    let v = 1.0 / ridge.max(1e-9);
    for i in 0..FEAT_RLS_DIM {
        for j in 0..FEAT_RLS_DIM {
            p[i][j] = if i == j { v } else { 0.0 };
        }
    }
}

fn feat_rls_update(
    beta: &mut [f64; FEAT_RLS_DIM],
    p: &mut [[f64; FEAT_RLS_DIM]; FEAT_RLS_DIM],
    x: &[f64; FEAT_RLS_DIM],
    y: f64,
    forgetting: f64,
    ridge: f64,
) {
    let lambda = forgetting.clamp(0.90, 0.9999);
    if p[0][0].abs() <= f64::EPSILON {
        feat_rls_init_p(p, ridge);
    }
    // P*x
    let mut px = [0.0; FEAT_RLS_DIM];
    for i in 0..FEAT_RLS_DIM {
        let mut v = 0.0;
        for j in 0..FEAT_RLS_DIM {
            v += p[i][j] * x[j];
        }
        px[i] = v;
    }
    // denom = lambda + x'*P*x
    let mut denom = lambda;
    for i in 0..FEAT_RLS_DIM {
        denom += x[i] * px[i];
    }
    if !denom.is_finite() || denom.abs() <= 1e-12 {
        return;
    }
    // Kalman gain k = P*x / denom
    let mut k = [0.0; FEAT_RLS_DIM];
    for i in 0..FEAT_RLS_DIM {
        k[i] = px[i] / denom;
    }
    // Update beta
    let err = y - feat_rls_dot(beta, x);
    for i in 0..FEAT_RLS_DIM {
        beta[i] += k[i] * err;
    }
    // Update P = (P - k * x' * P) / lambda
    let mut x_t_p = [0.0; FEAT_RLS_DIM];
    for j in 0..FEAT_RLS_DIM {
        let mut v = 0.0;
        for i in 0..FEAT_RLS_DIM {
            v += x[i] * p[i][j];
        }
        x_t_p[j] = v;
    }
    let mut next_p = [[0.0; FEAT_RLS_DIM]; FEAT_RLS_DIM];
    for i in 0..FEAT_RLS_DIM {
        for j in 0..FEAT_RLS_DIM {
            next_p[i][j] = (p[i][j] - k[i] * x_t_p[j]) / lambda;
        }
    }
    *p = next_p;
}

// ---------------------------------------------------------------------------
// CrossAssetMacroRls: cross-asset linear predictor with macro factors.
// Factors are inferred from symbol names:
// - S&P proxy: contains SPX/SP500/US500/SPY
// - Gold proxy: contains XAU/GOLD/GC
// - Oil proxy: contains WTI/BRENT/CL
// ---------------------------------------------------------------------------

const XASSET_DIM: usize = 5;

#[derive(Debug, Clone, Copy)]
pub struct CrossAssetMacroRlsYModelConfig {
    pub alpha_factor: f64,
    pub alpha_resid: f64,
    pub forgetting: f64,
    pub ridge: f64,
    pub pred_clip: f64,
    pub min_sigma: f64,
}

impl Default for CrossAssetMacroRlsYModelConfig {
    fn default() -> Self {
        Self {
            alpha_factor: 0.10,
            alpha_resid: 0.08,
            forgetting: 0.998,
            ridge: 0.05,
            pred_clip: 2.5,
            min_sigma: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MacroFactorState {
    last_prices: [Option<f64>; 3],
    rets_ewma: [f64; 3],
    seen: [bool; 3],
}

impl Default for MacroFactorState {
    fn default() -> Self {
        Self {
            last_prices: [None, None, None],
            rets_ewma: [0.0; 3],
            seen: [false; 3],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CrossAssetMacroState {
    last_price: Option<f64>,
    resid2: f64,
    has_stats: bool,
    last_x: [f64; XASSET_DIM],
    has_last_x: bool,
    beta: [f64; XASSET_DIM],
    p: [[f64; XASSET_DIM]; XASSET_DIM],
    samples: u64,
}

impl Default for CrossAssetMacroState {
    fn default() -> Self {
        Self {
            last_price: None,
            resid2: 0.0,
            has_stats: false,
            last_x: [0.0; XASSET_DIM],
            has_last_x: false,
            beta: [0.0; XASSET_DIM],
            p: [[0.0; XASSET_DIM]; XASSET_DIM],
            samples: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct CrossAssetMacroRlsYModel {
    cfg: CrossAssetMacroRlsYModelConfig,
    factor_state: MacroFactorState,
    by_instrument: HashMap<String, CrossAssetMacroState>,
    by_scope_side: HashMap<String, CrossAssetMacroState>,
}

impl CrossAssetMacroRlsYModel {
    pub fn new(cfg: CrossAssetMacroRlsYModelConfig) -> Self {
        Self {
            cfg,
            factor_state: MacroFactorState::default(),
            by_instrument: HashMap::new(),
            by_scope_side: HashMap::new(),
        }
    }

    pub fn observe_price(&mut self, instrument: &str, price: f64) {
        self.observe_factor_if_applicable(instrument, price);
        let snapshot = self.factor_state;
        Self::update_target_state(
            self.by_instrument
                .entry(instrument.to_string())
                .or_default(),
            snapshot,
            price,
            self.cfg,
        );
    }

    pub fn observe_signal_price(
        &mut self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        price: f64,
    ) {
        self.observe_factor_if_applicable(instrument, price);
        let snapshot = self.factor_state;
        let key = scoped_side_key(instrument, source_tag, signal);
        Self::update_target_state(
            self.by_scope_side.entry(key).or_default(),
            snapshot,
            price,
            self.cfg,
        );
    }

    pub fn estimate_base(
        &self,
        instrument: &str,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let Some(st) = self.by_instrument.get(instrument) else {
            return YNormal {
                mu: fallback_mu,
                sigma: fallback_sigma.max(self.cfg.min_sigma),
            };
        };
        xasset_estimate(st, self.cfg, fallback_mu, fallback_sigma)
    }

    pub fn estimate_for_signal(
        &self,
        instrument: &str,
        source_tag: &str,
        signal: &Signal,
        fallback_mu: f64,
        fallback_sigma: f64,
    ) -> YNormal {
        let base = self.estimate_base(instrument, fallback_mu, fallback_sigma);
        let key = scoped_side_key(instrument, source_tag, signal);
        let Some(scoped) = self.by_scope_side.get(&key) else {
            return base;
        };
        if scoped.samples == 0 {
            return base;
        }
        let scoped_est = xasset_estimate(scoped, self.cfg, base.mu, base.sigma);
        let n = scoped.samples as f64;
        let w = n / (n + 20.0);
        let mu = w * scoped_est.mu + (1.0 - w) * base.mu;
        let sigma = (w * scoped_est.sigma + (1.0 - w) * base.sigma).max(self.cfg.min_sigma);
        YNormal { mu, sigma }
    }

    fn observe_factor_if_applicable(&mut self, instrument: &str, price: f64) {
        if price <= f64::EPSILON {
            return;
        }
        let canonical = canonical_asset_symbol(instrument);
        let Some(idx) = macro_factor_index(&canonical) else {
            return;
        };
        if let Some(prev) = self.factor_state.last_prices[idx] {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                let a = self.cfg.alpha_factor.clamp(0.0, 1.0);
                self.factor_state.rets_ewma[idx] = if self.factor_state.seen[idx] {
                    (1.0 - a) * self.factor_state.rets_ewma[idx] + a * r
                } else {
                    r
                };
                self.factor_state.seen[idx] = true;
            }
        }
        self.factor_state.last_prices[idx] = Some(price);
    }

    fn update_target_state(
        st: &mut CrossAssetMacroState,
        factors: MacroFactorState,
        price: f64,
        cfg: CrossAssetMacroRlsYModelConfig,
    ) {
        if price <= f64::EPSILON {
            return;
        }
        if let Some(prev) = st.last_price {
            if prev > f64::EPSILON {
                let r = (price / prev).ln();
                if st.has_last_x {
                    let y_hat = xasset_dot(&st.beta, &st.last_x);
                    let err = r - y_hat;
                    xasset_rls_update(
                        &mut st.beta,
                        &mut st.p,
                        &st.last_x,
                        r,
                        cfg.forgetting,
                        cfg.ridge,
                    );
                    let a = cfg.alpha_resid.clamp(0.0, 1.0);
                    st.resid2 = if st.has_stats {
                        (1.0 - a) * st.resid2 + a * (err * err)
                    } else {
                        err * err
                    };
                    st.has_stats = true;
                    st.samples = st.samples.saturating_add(1);
                } else {
                    xasset_init_p(&mut st.p, cfg.ridge);
                }
                st.last_x = xasset_features(factors);
                st.has_last_x = true;
                if st.samples == 0 {
                    st.samples = 1;
                }
            }
        }
        st.last_price = Some(price);
    }
}

fn canonical_asset_symbol(instrument: &str) -> String {
    let upper = instrument.trim().to_ascii_uppercase();
    upper
        .replace(" (FUT)", "")
        .replace("#FUT", "")
        .replace(" ", "")
}

fn macro_factor_index(sym: &str) -> Option<usize> {
    if sym.contains("SPX") || sym.contains("SP500") || sym.contains("US500") || sym.contains("SPY")
    {
        return Some(0);
    }
    if sym.contains("XAU") || sym.contains("GOLD") || sym.contains("GC") {
        return Some(1);
    }
    if sym.contains("WTI") || sym.contains("BRENT") || sym.contains("CL") || sym.contains("OIL") {
        return Some(2);
    }
    None
}

fn xasset_features(f: MacroFactorState) -> [f64; XASSET_DIM] {
    let sp = if f.seen[0] { f.rets_ewma[0] } else { 0.0 };
    let gold = if f.seen[1] { f.rets_ewma[1] } else { 0.0 };
    let oil = if f.seen[2] { f.rets_ewma[2] } else { 0.0 };
    let mean = (sp + gold + oil) / 3.0;
    let disp = (((sp - mean).powi(2) + (gold - mean).powi(2) + (oil - mean).powi(2)) / 3.0).sqrt();
    [1.0, sp, gold, oil, disp]
}

fn xasset_estimate(
    st: &CrossAssetMacroState,
    cfg: CrossAssetMacroRlsYModelConfig,
    fallback_mu: f64,
    fallback_sigma: f64,
) -> YNormal {
    if !st.has_last_x {
        return YNormal {
            mu: fallback_mu,
            sigma: fallback_sigma.max(cfg.min_sigma),
        };
    }
    let pred = xasset_dot(&st.beta, &st.last_x);
    let sigma_model = st.resid2.max(0.0).sqrt().max(cfg.min_sigma);
    let pred_clipped = pred.clamp(-cfg.pred_clip * sigma_model, cfg.pred_clip * sigma_model);
    let n = st.samples as f64;
    let w = n / (n + 100.0);
    let mu = w * pred_clipped + (1.0 - w) * fallback_mu;
    let sigma =
        (w * sigma_model + (1.0 - w) * fallback_sigma.max(cfg.min_sigma)).max(cfg.min_sigma);
    YNormal { mu, sigma }
}

fn xasset_dot(a: &[f64; XASSET_DIM], b: &[f64; XASSET_DIM]) -> f64 {
    let mut s = 0.0;
    for i in 0..XASSET_DIM {
        s += a[i] * b[i];
    }
    s
}

fn xasset_init_p(p: &mut [[f64; XASSET_DIM]; XASSET_DIM], ridge: f64) {
    let v = 1.0 / ridge.max(1e-9);
    for (i, row) in p.iter_mut().enumerate().take(XASSET_DIM) {
        for (j, cell) in row.iter_mut().enumerate().take(XASSET_DIM) {
            *cell = if i == j { v } else { 0.0 };
        }
    }
}

fn xasset_rls_update(
    beta: &mut [f64; XASSET_DIM],
    p: &mut [[f64; XASSET_DIM]; XASSET_DIM],
    x: &[f64; XASSET_DIM],
    y: f64,
    forgetting: f64,
    ridge: f64,
) {
    let lambda = forgetting.clamp(0.90, 0.9999);
    if p[0][0].abs() <= f64::EPSILON {
        xasset_init_p(p, ridge);
    }
    let mut px = [0.0; XASSET_DIM];
    for (i, px_i) in px.iter_mut().enumerate().take(XASSET_DIM) {
        let mut v = 0.0;
        for (j, xj) in x.iter().enumerate().take(XASSET_DIM) {
            v += p[i][j] * *xj;
        }
        *px_i = v;
    }
    let mut denom = lambda;
    for (i, x_i) in x.iter().enumerate().take(XASSET_DIM) {
        denom += *x_i * px[i];
    }
    if !denom.is_finite() || denom.abs() <= 1e-12 {
        return;
    }
    let mut k = [0.0; XASSET_DIM];
    for i in 0..XASSET_DIM {
        k[i] = px[i] / denom;
    }
    let err = y - xasset_dot(beta, x);
    for i in 0..XASSET_DIM {
        beta[i] += k[i] * err;
    }
    let mut x_t_p = [0.0; XASSET_DIM];
    for (j, xtpj) in x_t_p.iter_mut().enumerate().take(XASSET_DIM) {
        let mut v = 0.0;
        for (i, x_i) in x.iter().enumerate().take(XASSET_DIM) {
            v += *x_i * p[i][j];
        }
        *xtpj = v;
    }
    let mut next_p = [[0.0; XASSET_DIM]; XASSET_DIM];
    for i in 0..XASSET_DIM {
        for (j, xtpj) in x_t_p.iter().enumerate().take(XASSET_DIM) {
            next_p[i][j] = (p[i][j] - k[i] * *xtpj) / lambda;
        }
    }
    *p = next_p;
}

fn scoped_side_key(instrument: &str, source_tag: &str, signal: &Signal) -> String {
    let side = match signal {
        Signal::Buy => "buy",
        Signal::Sell => "sell",
        Signal::Hold => "hold",
    };
    format!(
        "{}::{}::{}",
        instrument.trim().to_ascii_uppercase(),
        source_tag.trim().to_ascii_lowercase(),
        side
    )
}

#[derive(Debug, Clone, Copy)]
pub struct PendingPrediction {
    pub due_ms: u64,
    pub base_price: f64,
    pub mu: f64,
    pub norm_scale: f64,
}

#[derive(Debug, Clone)]
pub struct OnlinePredictorMetrics {
    window: usize,
    pairs: VecDeque<(f64, f64)>,
}

impl Default for OnlinePredictorMetrics {
    fn default() -> Self {
        Self {
            window: PREDICTOR_METRIC_WINDOW,
            pairs: VecDeque::with_capacity(PREDICTOR_METRIC_WINDOW),
        }
    }
}

impl OnlinePredictorMetrics {
    pub fn with_window(window: usize) -> Self {
        Self {
            window: window.max(2),
            pairs: VecDeque::with_capacity(window.max(2)),
        }
    }

    pub fn observe(&mut self, y_real: f64, y_pred: f64) {
        if !y_real.is_finite() || !y_pred.is_finite() {
            return;
        }
        self.pairs.push_back((y_real, y_pred));
        if self.pairs.len() > self.window {
            let _ = self.pairs.pop_front();
        }
    }

    pub fn sample_count(&self) -> u64 {
        self.pairs.len() as u64
    }

    pub fn mae(&self) -> Option<f64> {
        let n = self.pairs.len();
        if n == 0 {
            return None;
        }
        let sum_abs = self
            .pairs
            .iter()
            .map(|(y, yhat)| (y - yhat).abs())
            .sum::<f64>();
        Some(sum_abs / n as f64)
    }

    pub fn hit_rate(&self) -> Option<f64> {
        let n = self.pairs.len();
        if n == 0 {
            return None;
        }
        let hit = self.pairs.iter().filter(|(y, yhat)| y * yhat > 0.0).count() as f64;
        Some(hit / n as f64)
    }

    pub fn r2(&self) -> Option<f64> {
        let n = self.pairs.len();
        if n < PREDICTOR_R2_MIN_SAMPLES {
            return None;
        }
        let mean_y = self.pairs.iter().map(|(y, _)| *y).sum::<f64>() / n as f64;
        let mut sse = 0.0;
        let mut sst = 0.0;
        for (y, yhat) in &self.pairs {
            let err = y - yhat;
            sse += err * err;
            let d = y - mean_y;
            sst += d * d;
        }
        if sst <= 1e-18 {
            return Some(0.0);
        }
        Some(1.0 - (sse / sst))
    }
}

pub fn stride_closes(closes: &[f64], stride: usize) -> Vec<f64> {
    if stride <= 1 {
        return closes.to_vec();
    }
    closes.iter().step_by(stride).copied().collect()
}

pub fn backfill_predictor_metrics_from_closes(
    closes: &[f64],
    alpha_mean: f64,
    window: usize,
) -> OnlinePredictorMetrics {
    let mut out = OnlinePredictorMetrics::with_window(window);
    let mut prev: Option<f64> = None;
    let mut has_mu = false;
    let mut mu = 0.0;
    let a = alpha_mean.clamp(0.0, 1.0);
    for p in closes {
        if *p <= f64::EPSILON {
            continue;
        }
        if let Some(pp) = prev {
            if pp > f64::EPSILON {
                let r = (p / pp).ln();
                let pred = if has_mu { mu } else { 0.0 };
                out.observe(r, pred);
                if !has_mu {
                    mu = r;
                    has_mu = true;
                } else {
                    mu = (1.0 - a) * mu + a * r;
                }
            }
        }
        prev = Some(*p);
    }
    out
}

pub fn backfill_predictor_metrics_from_closes_volnorm(
    closes: &[f64],
    alpha_mean: f64,
    alpha_var: f64,
    min_sigma: f64,
    window: usize,
) -> OnlinePredictorMetrics {
    let mut out = OnlinePredictorMetrics::with_window(window);
    let mut prev: Option<f64> = None;
    let mut has_mu = false;
    let mut mu: f64 = 0.0;
    let mut var: f64 = 0.0;
    let mut has_var = false;
    let a_mu = alpha_mean.clamp(0.0, 1.0);
    let a_var = alpha_var.clamp(0.0, 1.0);
    let sigma_floor = min_sigma.max(1e-8);

    for p in closes {
        if *p <= f64::EPSILON {
            continue;
        }
        if let Some(pp) = prev {
            if pp > f64::EPSILON {
                let r = (p / pp).ln();
                let pred = if has_mu { mu } else { 0.0 };
                let sigma = if has_var {
                    var.max(0.0).sqrt().max(sigma_floor)
                } else {
                    sigma_floor
                };
                out.observe(r / sigma, pred / sigma);

                if !has_mu {
                    mu = r;
                    has_mu = true;
                    var = r * r;
                    has_var = true;
                } else {
                    let prev_mu = mu;
                    mu = (1.0 - a_mu) * mu + a_mu * r;
                    let centered = r - prev_mu;
                    let sample_var = centered * centered;
                    if !has_var {
                        var = sample_var;
                        has_var = true;
                    } else {
                        var = (1.0 - a_var) * var + a_var * sample_var;
                    }
                }
            }
        }
        prev = Some(*p);
    }
    out
}

pub fn predictor_metrics_scope_key(
    symbol: &str,
    market: MarketKind,
    predictor: &str,
    horizon: &str,
) -> String {
    let market_label = if market == MarketKind::Futures {
        "futures"
    } else {
        "spot"
    };
    format!(
        "{}::{}::{}::{}",
        symbol.trim().to_ascii_uppercase(),
        market_label,
        predictor.trim().to_ascii_lowercase(),
        horizon.trim().to_ascii_lowercase(),
    )
}

pub fn parse_predictor_metrics_scope_key(key: &str) -> Option<(String, String, String, String)> {
    let mut it = key.splitn(4, "::");
    let symbol = it.next()?.to_string();
    let market = it.next()?.to_string();
    let predictor = it.next()?.to_string();
    let horizon = it.next()?.to_string();
    Some((symbol, market, predictor, horizon))
}
