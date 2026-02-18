use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyProfile {
    pub label: String,
    pub source_tag: String,
    #[serde(default)]
    pub symbol: String,
    pub fast_period: usize,
    pub slow_period: usize,
    pub min_ticks_between_signals: u64,
}

impl StrategyProfile {
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
    pub fn new(
        default_symbol: &str,
        config_fast: usize,
        config_slow: usize,
        min_ticks_between_signals: u64,
    ) -> Self {
        let symbol = default_symbol.trim().to_ascii_uppercase();
        Self {
            profiles: vec![
                StrategyProfile {
                    label: "MA(Config)".to_string(),
                    source_tag: "cfg".to_string(),
                    symbol: symbol.clone(),
                    fast_period: config_fast,
                    slow_period: config_slow,
                    min_ticks_between_signals,
                },
                StrategyProfile {
                    label: "MA(Fast 5/20)".to_string(),
                    source_tag: "fst".to_string(),
                    symbol: symbol.clone(),
                    fast_period: 5,
                    slow_period: 20,
                    min_ticks_between_signals,
                },
                StrategyProfile {
                    label: "MA(Slow 20/60)".to_string(),
                    source_tag: "slw".to_string(),
                    symbol,
                    fast_period: 20,
                    slow_period: 60,
                    min_ticks_between_signals,
                },
            ],
            next_custom_id: 1,
        }
    }

    pub fn labels(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.label.clone()).collect()
    }

    pub fn symbols(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.symbol.clone()).collect()
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

    pub fn profiles(&self) -> &[StrategyProfile] {
        &self.profiles
    }

    pub fn index_of_label(&self, label: &str) -> Option<usize> {
        self.profiles.iter().position(|p| p.label == label)
    }

    pub fn from_profiles(
        mut profiles: Vec<StrategyProfile>,
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
        for profile in &mut profiles {
            if profile.symbol.trim().is_empty() {
                profile.symbol = default_symbol.trim().to_ascii_uppercase();
            }
        }
        let next_custom_id = profiles
            .iter()
            .filter_map(|profile| {
                let tag = profile.source_tag.strip_prefix('c')?;
                tag.parse::<u32>().ok()
            })
            .max()
            .map(|id| id + 1)
            .unwrap_or(1);
        Self {
            profiles,
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
            &base.symbol,
            base.fast_period,
            base.slow_period,
            base.min_ticks_between_signals,
        )
    }

    pub fn fork_profile(
        &mut self,
        index: usize,
        symbol: &str,
        fast_period: usize,
        slow_period: usize,
        min_ticks_between_signals: u64,
    ) -> Option<StrategyProfile> {
        self.profiles.get(index)?;
        Some(self.new_custom_profile(
            symbol,
            fast_period,
            slow_period,
            min_ticks_between_signals,
        ))
    }

    fn new_custom_profile(
        &mut self,
        symbol: &str,
        fast_period: usize,
        slow_period: usize,
        min_ticks_between_signals: u64,
    ) -> StrategyProfile {
        let tag = format!("c{:02}", self.next_custom_id);
        self.next_custom_id += 1;
        let fast = fast_period.max(2);
        let slow = slow_period.max(fast + 1);
        let profile = StrategyProfile {
            label: format!("MA(Custom {}/{}) [{}]", fast, slow, tag),
            source_tag: tag,
            symbol: symbol.trim().to_ascii_uppercase(),
            fast_period: fast,
            slow_period: slow,
            min_ticks_between_signals: min_ticks_between_signals.max(1),
        };
        self.profiles.push(profile.clone());
        profile
    }
}
