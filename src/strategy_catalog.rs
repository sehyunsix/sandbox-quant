use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyKind {
    Ma,
    Ema,
    Atr,
    Vlc,
    Chb,
    Orb,
    Rsa,
    Dct,
    Mrv,
    Bbr,
    Sto,
    Reg,
    Ens,
    Mac,
    Roc,
    Arn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrategyKindSpec {
    pub kind: StrategyKind,
    pub label: &'static str,
    pub category: &'static str,
    pub default_fast: usize,
    pub default_slow: usize,
    pub default_cooldown: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyTypeOption {
    pub display_label: String,
    pub strategy_label: Option<String>,
}

const STRATEGY_KIND_SPECS: [StrategyKindSpec; 16] = [
    StrategyKindSpec {
        kind: StrategyKind::Ma,
        label: "MA",
        category: "Trend",
        default_fast: 5,
        default_slow: 20,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Ema,
        label: "EMA",
        category: "Trend",
        default_fast: 9,
        default_slow: 21,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Atr,
        label: "ATR",
        category: "Volatility",
        default_fast: 14,
        default_slow: 180,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Vlc,
        label: "VLC",
        category: "Volatility",
        default_fast: 20,
        default_slow: 120,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Chb,
        label: "CHB",
        category: "Breakout",
        default_fast: 20,
        default_slow: 10,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Orb,
        label: "ORB",
        category: "Breakout",
        default_fast: 12,
        default_slow: 8,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Rsa,
        label: "RSA",
        category: "MeanReversion",
        default_fast: 14,
        default_slow: 70,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Dct,
        label: "DCT",
        category: "Trend",
        default_fast: 20,
        default_slow: 10,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Mrv,
        label: "MRV",
        category: "MeanReversion",
        default_fast: 20,
        default_slow: 200,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Bbr,
        label: "BBR",
        category: "MeanReversion",
        default_fast: 20,
        default_slow: 200,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Sto,
        label: "STO",
        category: "MeanReversion",
        default_fast: 14,
        default_slow: 80,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Reg,
        label: "REG",
        category: "Hybrid",
        default_fast: 10,
        default_slow: 30,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Ens,
        label: "ENS",
        category: "Hybrid",
        default_fast: 10,
        default_slow: 30,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Mac,
        label: "MAC",
        category: "Trend",
        default_fast: 12,
        default_slow: 26,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Roc,
        label: "ROC",
        category: "Trend",
        default_fast: 10,
        default_slow: 20,
        default_cooldown: 1,
    },
    StrategyKindSpec {
        kind: StrategyKind::Arn,
        label: "ARN",
        category: "Trend",
        default_fast: 14,
        default_slow: 70,
        default_cooldown: 1,
    },
];
const STRATEGY_CATEGORY_ORDER: [&str; 5] = [
    "Trend",
    "MeanReversion",
    "Volatility",
    "Breakout",
    "Hybrid",
];

impl StrategyKind {
    pub fn specs() -> &'static [StrategyKindSpec] {
        &STRATEGY_KIND_SPECS
    }

    pub fn as_label(self) -> &'static str {
        Self::specs()
            .iter()
            .find(|spec| spec.kind == self)
            .map(|spec| spec.label)
            .unwrap_or("MA")
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::specs()
            .iter()
            .find(|spec| spec.label.eq_ignore_ascii_case(label))
            .map(|spec| spec.kind)
    }

    pub fn defaults(self) -> (usize, usize, u64) {
        let spec = Self::specs()
            .iter()
            .find(|spec| spec.kind == self)
            .copied()
            .unwrap_or(STRATEGY_KIND_SPECS[0]);
        (spec.default_fast, spec.default_slow, spec.default_cooldown)
    }
}

pub fn strategy_kind_labels() -> Vec<String> {
    StrategyKind::specs()
        .iter()
        .map(|spec| spec.label.to_string())
        .collect()
}

pub fn strategy_kind_categories() -> Vec<String> {
    STRATEGY_CATEGORY_ORDER
        .iter()
        .map(|item| item.to_string())
        .collect()
}

pub fn strategy_kind_labels_by_category(category: &str) -> Vec<String> {
    StrategyKind::specs()
        .iter()
        .filter(|spec| spec.category.eq_ignore_ascii_case(category))
        .map(|spec| spec.label.to_string())
        .collect()
}

pub fn strategy_kind_category_for_label(label: &str) -> Option<String> {
    StrategyKind::specs()
        .iter()
        .find(|spec| spec.label.eq_ignore_ascii_case(label))
        .map(|spec| spec.category.to_string())
}

pub fn strategy_type_options_by_category(category: &str) -> Vec<StrategyTypeOption> {
    let mut options: Vec<StrategyTypeOption> = StrategyKind::specs()
        .iter()
        .filter(|spec| spec.category.eq_ignore_ascii_case(category))
        .map(|spec| StrategyTypeOption {
            display_label: spec.label.to_string(),
            strategy_label: Some(spec.label.to_string()),
        })
        .collect();
    let coming_soon: &[&str] = &[];
    options.extend(coming_soon.iter().map(|name| StrategyTypeOption {
        display_label: format!("{} (Coming soon)", name),
        strategy_label: None,
    }));
    options
}

#[derive(Debug, Clone)]
pub struct StrategyLifecycleRow {
    pub created_at_ms: i64,
    pub total_running_ms: u64,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfile {
    pub label: String,
    pub source_tag: String,
    #[serde(default)]
    pub strategy_type: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default = "now_ms")]
    pub created_at_ms: i64,
    #[serde(default)]
    pub cumulative_running_ms: u64,
    #[serde(default)]
    pub last_started_at_ms: Option<i64>,
    pub fast_period: usize,
    pub slow_period: usize,
    pub min_ticks_between_signals: u64,
}

impl StrategyProfile {
    pub fn strategy_type_id(&self) -> String {
        let ty = self.strategy_type.trim().to_ascii_lowercase();
        if ty.is_empty() {
            self.strategy_kind().as_label().to_ascii_lowercase()
        } else {
            ty
        }
    }

    pub fn strategy_kind(&self) -> StrategyKind {
        match self.strategy_type.trim().to_ascii_lowercase().as_str() {
            "arn" => return StrategyKind::Arn,
            "roc" => return StrategyKind::Roc,
            "mac" => return StrategyKind::Mac,
            "ens" => return StrategyKind::Ens,
            "reg" => return StrategyKind::Reg,
            "orb" => return StrategyKind::Orb,
            "vlc" => return StrategyKind::Vlc,
            "sto" => return StrategyKind::Sto,
            "bbr" => return StrategyKind::Bbr,
            "mrv" => return StrategyKind::Mrv,
            "dct" => return StrategyKind::Dct,
            "rsa" => return StrategyKind::Rsa,
            "chb" => return StrategyKind::Chb,
            "atr" => return StrategyKind::Atr,
            "ema" => return StrategyKind::Ema,
            "ma" => return StrategyKind::Ma,
            _ => {}
        }
        if self.source_tag.eq_ignore_ascii_case("arn")
            || self.label.to_ascii_uppercase().starts_with("ARN(")
        {
            StrategyKind::Arn
        } else if self.source_tag.eq_ignore_ascii_case("roc")
            || self.label.to_ascii_uppercase().starts_with("ROC(")
        {
            StrategyKind::Roc
        } else if self.source_tag.eq_ignore_ascii_case("mac")
            || self.label.to_ascii_uppercase().starts_with("MAC(")
        {
            StrategyKind::Mac
        } else if self.source_tag.eq_ignore_ascii_case("ens")
            || self.label.to_ascii_uppercase().starts_with("ENS(")
        {
            StrategyKind::Ens
        } else if self.source_tag.eq_ignore_ascii_case("reg")
            || self.label.to_ascii_uppercase().starts_with("REG(")
        {
            StrategyKind::Reg
        } else if self.source_tag.eq_ignore_ascii_case("rsa")
            || self.label.to_ascii_uppercase().starts_with("RSA(")
        {
            StrategyKind::Rsa
        } else if self.source_tag.eq_ignore_ascii_case("vlc")
            || self.label.to_ascii_uppercase().starts_with("VLC(")
        {
            StrategyKind::Vlc
        } else if self.source_tag.eq_ignore_ascii_case("orb")
            || self.label.to_ascii_uppercase().starts_with("ORB(")
        {
            StrategyKind::Orb
        } else if self.source_tag.eq_ignore_ascii_case("bbr")
            || self.label.to_ascii_uppercase().starts_with("BBR(")
        {
            StrategyKind::Bbr
        } else if self.source_tag.eq_ignore_ascii_case("sto")
            || self.label.to_ascii_uppercase().starts_with("STO(")
        {
            StrategyKind::Sto
        } else if self.source_tag.eq_ignore_ascii_case("dct")
            || self.label.to_ascii_uppercase().starts_with("DCT(")
        {
            StrategyKind::Dct
        } else if self.source_tag.eq_ignore_ascii_case("mrv")
            || self.label.to_ascii_uppercase().starts_with("MRV(")
        {
            StrategyKind::Mrv
        } else if self.source_tag.eq_ignore_ascii_case("atr")
            || self.label.to_ascii_uppercase().starts_with("ATR(")
            || self.label.to_ascii_uppercase().starts_with("ATRX(")
        {
            StrategyKind::Atr
        } else if self.source_tag.eq_ignore_ascii_case("chb")
            || self.label.to_ascii_uppercase().starts_with("CHB(")
        {
            StrategyKind::Chb
        } else if self.source_tag.eq_ignore_ascii_case("ema")
            || self.label.to_ascii_uppercase().starts_with("EMA(")
        {
            StrategyKind::Ema
        } else {
            StrategyKind::Ma
        }
    }

    pub fn periods_tuple(&self) -> (usize, usize, u64) {
        (
            self.fast_period,
            self.slow_period,
            self.min_ticks_between_signals,
        )
    }
}

#[derive(Debug, Clone)]
pub struct StrategyCatalog {
    profiles: Vec<StrategyProfile>,
    next_custom_id: u32,
}

impl StrategyCatalog {
    fn builtin_profiles(
        default_symbol: &str,
        config_fast: usize,
        config_slow: usize,
        min_ticks_between_signals: u64,
    ) -> Vec<StrategyProfile> {
        let symbol = default_symbol.trim().to_ascii_uppercase();
        vec![
            StrategyProfile {
                label: "MA(Config)".to_string(),
                source_tag: "cfg".to_string(),
                strategy_type: "ma".to_string(),
                symbol: symbol.clone(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: config_fast,
                slow_period: config_slow,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "MA(Fast 5/20)".to_string(),
                source_tag: "fst".to_string(),
                strategy_type: "ma".to_string(),
                symbol: symbol.clone(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 5,
                slow_period: 20,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "MA(Slow 20/60)".to_string(),
                source_tag: "slw".to_string(),
                strategy_type: "ma".to_string(),
                symbol: symbol.clone(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 20,
                slow_period: 60,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "RSA(RSI 14 30/70)".to_string(),
                source_tag: "rsa".to_string(),
                strategy_type: "rsa".to_string(),
                symbol,
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 14,
                slow_period: 70,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "DCT(Donchian 20/10)".to_string(),
                source_tag: "dct".to_string(),
                strategy_type: "dct".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 20,
                slow_period: 10,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "MRV(SMA 20 -2.00%)".to_string(),
                source_tag: "mrv".to_string(),
                strategy_type: "mrv".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 20,
                slow_period: 200,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "BBR(BB 20 2.00x)".to_string(),
                source_tag: "bbr".to_string(),
                strategy_type: "bbr".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 20,
                slow_period: 200,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "STO(Stoch 14 20/80)".to_string(),
                source_tag: "sto".to_string(),
                strategy_type: "sto".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 14,
                slow_period: 80,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "VLC(Compression 20 1.20%)".to_string(),
                source_tag: "vlc".to_string(),
                strategy_type: "vlc".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 20,
                slow_period: 120,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "ORB(Opening 12/8)".to_string(),
                source_tag: "orb".to_string(),
                strategy_type: "orb".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 12,
                slow_period: 8,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "REG(Regime 10/30)".to_string(),
                source_tag: "reg".to_string(),
                strategy_type: "reg".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 10,
                slow_period: 30,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "ENS(Vote 10/30)".to_string(),
                source_tag: "ens".to_string(),
                strategy_type: "ens".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 10,
                slow_period: 30,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "MAC(MACD 12/26)".to_string(),
                source_tag: "mac".to_string(),
                strategy_type: "mac".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 12,
                slow_period: 26,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "ROC(ROC 10 0.20%)".to_string(),
                source_tag: "roc".to_string(),
                strategy_type: "roc".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 10,
                slow_period: 20,
                min_ticks_between_signals,
            },
            StrategyProfile {
                label: "ARN(Aroon 14 70)".to_string(),
                source_tag: "arn".to_string(),
                strategy_type: "arn".to_string(),
                symbol: default_symbol.trim().to_ascii_uppercase(),
                created_at_ms: now_ms(),
                cumulative_running_ms: 0,
                last_started_at_ms: None,
                fast_period: 14,
                slow_period: 70,
                min_ticks_between_signals,
            },
        ]
    }

    pub fn is_custom_source_tag(source_tag: &str) -> bool {
        let Some(id) = source_tag.strip_prefix('c') else {
            return false;
        };
        !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit())
    }

    pub fn new(
        default_symbol: &str,
        config_fast: usize,
        config_slow: usize,
        min_ticks_between_signals: u64,
    ) -> Self {
        Self {
            profiles: Self::builtin_profiles(
                default_symbol,
                config_fast,
                config_slow,
                min_ticks_between_signals,
            ),
            next_custom_id: 1,
        }
    }

    pub fn labels(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.label.clone()).collect()
    }

    pub fn symbols(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.symbol.clone()).collect()
    }

    pub fn lifecycle_rows(&self, now_ms: i64) -> Vec<StrategyLifecycleRow> {
        self.profiles
            .iter()
            .map(|profile| {
                let running_delta = profile
                    .last_started_at_ms
                    .map(|started| now_ms.saturating_sub(started).max(0) as u64)
                    .unwrap_or(0);
                StrategyLifecycleRow {
                    created_at_ms: profile.created_at_ms,
                    total_running_ms: profile.cumulative_running_ms.saturating_add(running_delta),
                }
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn get(&self, index: usize) -> Option<&StrategyProfile> {
        self.profiles.get(index)
    }

    pub fn get_by_source_tag(&self, source_tag: &str) -> Option<&StrategyProfile> {
        self.profiles.iter().find(|p| p.source_tag == source_tag)
    }

    pub fn mark_running(&mut self, source_tag: &str, now_ms: i64) -> bool {
        let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|p| p.source_tag == source_tag)
        else {
            return false;
        };
        if profile.last_started_at_ms.is_none() {
            profile.last_started_at_ms = Some(now_ms);
        }
        true
    }

    pub fn mark_stopped(&mut self, source_tag: &str, now_ms: i64) -> bool {
        let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|p| p.source_tag == source_tag)
        else {
            return false;
        };
        if let Some(started) = profile.last_started_at_ms.take() {
            let delta = now_ms.saturating_sub(started).max(0) as u64;
            profile.cumulative_running_ms = profile.cumulative_running_ms.saturating_add(delta);
        }
        true
    }

    pub fn stop_all_running(&mut self, now_ms: i64) {
        for profile in &mut self.profiles {
            if let Some(started) = profile.last_started_at_ms.take() {
                let delta = now_ms.saturating_sub(started).max(0) as u64;
                profile.cumulative_running_ms = profile.cumulative_running_ms.saturating_add(delta);
            }
        }
    }

    pub fn profiles(&self) -> &[StrategyProfile] {
        &self.profiles
    }

    pub fn index_of_label(&self, label: &str) -> Option<usize> {
        self.profiles.iter().position(|p| p.label == label)
    }

    pub fn from_profiles(
        profiles: Vec<StrategyProfile>,
        default_symbol: &str,
        config_fast: usize,
        config_slow: usize,
        min_ticks_between_signals: u64,
    ) -> Self {
        if profiles.is_empty() {
            return Self::new(
                default_symbol,
                config_fast,
                config_slow,
                min_ticks_between_signals,
            );
        }
        let mut restored = profiles;
        let mut merged = Vec::new();
        for builtin in Self::builtin_profiles(
            default_symbol,
            config_fast,
            config_slow,
            min_ticks_between_signals,
        ) {
            if let Some(idx) = restored
                .iter()
                .position(|p| p.source_tag.eq_ignore_ascii_case(&builtin.source_tag))
            {
                merged.push(restored.remove(idx));
            } else {
                merged.push(builtin);
            }
        }
        merged.extend(restored);

        for profile in &mut merged {
            if profile.source_tag.trim().is_empty() {
                profile.source_tag = "cfg".to_string();
            } else {
                profile.source_tag = profile.source_tag.trim().to_ascii_lowercase();
            }
            if profile.strategy_type.trim().is_empty() {
                profile.strategy_type = profile.strategy_kind().as_label().to_ascii_lowercase();
            } else {
                profile.strategy_type = profile.strategy_type.trim().to_ascii_lowercase();
            }
            if profile.symbol.trim().is_empty() {
                profile.symbol = default_symbol.trim().to_ascii_uppercase();
            }
            if profile.created_at_ms <= 0 {
                profile.created_at_ms = now_ms();
            }
        }
        let next_custom_id = merged
            .iter()
            .filter_map(|profile| {
                let tag = profile.source_tag.strip_prefix('c')?;
                tag.parse::<u32>().ok()
            })
            .max()
            .map(|id| id + 1)
            .unwrap_or(1);
        Self {
            profiles: merged,
            next_custom_id,
        }
    }

    pub fn add_custom_from_index(&mut self, base_index: usize) -> StrategyProfile {
        let base = self
            .profiles
            .get(base_index)
            .cloned()
            .unwrap_or_else(|| self.profiles[0].clone());
        self.new_custom_profile(
            base.strategy_kind(),
            &base.symbol,
            base.fast_period,
            base.slow_period,
            base.min_ticks_between_signals,
        )
    }

    pub fn fork_profile(
        &mut self,
        index: usize,
        kind: StrategyKind,
        symbol: &str,
        fast_period: usize,
        slow_period: usize,
        min_ticks_between_signals: u64,
    ) -> Option<StrategyProfile> {
        self.profiles.get(index)?;
        Some(self.new_custom_profile(
            kind,
            symbol,
            fast_period,
            slow_period,
            min_ticks_between_signals,
        ))
    }

    pub fn remove_custom_profile(&mut self, index: usize) -> Option<StrategyProfile> {
        let profile = self.profiles.get(index)?;
        if !Self::is_custom_source_tag(&profile.source_tag) {
            return None;
        }
        Some(self.profiles.remove(index))
    }

    fn new_custom_profile(
        &mut self,
        kind: StrategyKind,
        symbol: &str,
        fast_period: usize,
        slow_period: usize,
        min_ticks_between_signals: u64,
    ) -> StrategyProfile {
        let tag = format!("c{:02}", self.next_custom_id);
        self.next_custom_id += 1;
        let fast = fast_period.max(2);
        let (label, slow) = match kind {
            StrategyKind::Ma => {
                let slow = slow_period.max(fast + 1);
                (format!("MA(Custom {}/{}) [{}]", fast, slow, tag), slow)
            }
            StrategyKind::Ema => {
                let slow = slow_period.max(fast + 1);
                (format!("EMA(Custom {}/{}) [{}]", fast, slow, tag), slow)
            }
            StrategyKind::Atr => {
                let threshold_x100 = slow_period.clamp(110, 500);
                (
                    format!("ATRX(Custom {} {:.2}x) [{}]", fast, threshold_x100 as f64 / 100.0, tag),
                    threshold_x100,
                )
            }
            StrategyKind::Vlc => {
                let threshold_bps = slow_period.clamp(10, 5000);
                (
                    format!(
                        "VLC(Custom {} {:.2}%) [{}]",
                        fast,
                        threshold_bps as f64 / 100.0,
                        tag
                    ),
                    threshold_bps,
                )
            }
            StrategyKind::Chb => {
                let exit_window = slow_period.max(2);
                (
                    format!("CHB(Custom {}/{}) [{}]", fast, exit_window, tag),
                    exit_window,
                )
            }
            StrategyKind::Orb => {
                let exit_window = slow_period.max(2);
                (
                    format!("ORB(Custom {}/{}) [{}]", fast, exit_window, tag),
                    exit_window,
                )
            }
            StrategyKind::Rsa => {
                let upper = slow_period.clamp(51, 95);
                let lower = 100 - upper;
                (
                    format!("RSA(Custom {} {}/{}) [{}]", fast, lower, upper, tag),
                    upper,
                )
            }
            StrategyKind::Dct => {
                let exit_window = slow_period.max(2);
                (
                    format!("DCT(Custom {}/{}) [{}]", fast, exit_window, tag),
                    exit_window,
                )
            }
            StrategyKind::Mrv => {
                let threshold_bps = slow_period.clamp(10, 3000);
                (
                    format!(
                        "MRV(Custom {} -{:.2}%) [{}]",
                        fast,
                        threshold_bps as f64 / 100.0,
                        tag
                    ),
                    threshold_bps,
                )
            }
            StrategyKind::Bbr => {
                let band_mult_x100 = slow_period.clamp(50, 400);
                (
                    format!(
                        "BBR(Custom {} {:.2}x) [{}]",
                        fast,
                        band_mult_x100 as f64 / 100.0,
                        tag
                    ),
                    band_mult_x100,
                )
            }
            StrategyKind::Sto => {
                let upper = slow_period.clamp(51, 95);
                let lower = 100 - upper;
                (
                    format!("STO(Custom {} {}/{}) [{}]", fast, lower, upper, tag),
                    upper,
                )
            }
            StrategyKind::Reg => {
                let slow = slow_period.max(fast + 1);
                (format!("REG(Custom {}/{}) [{}]", fast, slow, tag), slow)
            }
            StrategyKind::Ens => {
                let slow = slow_period.max(fast + 1);
                (format!("ENS(Custom {}/{}) [{}]", fast, slow, tag), slow)
            }
            StrategyKind::Mac => {
                let slow = slow_period.max(fast + 1);
                (format!("MAC(Custom {}/{}) [{}]", fast, slow, tag), slow)
            }
            StrategyKind::Roc => {
                let threshold_bps = slow_period.clamp(5, 1_000);
                (
                    format!(
                        "ROC(Custom {} {:.2}%) [{}]",
                        fast,
                        threshold_bps as f64 / 100.0,
                        tag
                    ),
                    threshold_bps,
                )
            }
            StrategyKind::Arn => {
                let threshold = slow_period.clamp(50, 90);
                (format!("ARN(Custom {} {}) [{}]", fast, threshold, tag), threshold)
            }
        };
        let profile = StrategyProfile {
            label,
            source_tag: tag,
            strategy_type: kind.as_label().to_ascii_lowercase(),
            symbol: symbol.trim().to_ascii_uppercase(),
            created_at_ms: now_ms(),
            cumulative_running_ms: 0,
            last_started_at_ms: None,
            fast_period: fast,
            slow_period: slow,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
        };
        self.profiles.push(profile.clone());
        profile
    }
}
