#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyProfile {
    pub label: String,
    pub source_tag: String,
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
    pub fn new(config_fast: usize, config_slow: usize, min_ticks_between_signals: u64) -> Self {
        Self {
            profiles: vec![
                StrategyProfile {
                    label: "MA(Config)".to_string(),
                    source_tag: "cfg".to_string(),
                    fast_period: config_fast,
                    slow_period: config_slow,
                    min_ticks_between_signals,
                },
                StrategyProfile {
                    label: "MA(Fast 5/20)".to_string(),
                    source_tag: "fst".to_string(),
                    fast_period: 5,
                    slow_period: 20,
                    min_ticks_between_signals,
                },
                StrategyProfile {
                    label: "MA(Slow 20/60)".to_string(),
                    source_tag: "slw".to_string(),
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

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn get(&self, index: usize) -> Option<&StrategyProfile> {
        self.profiles.get(index)
    }

    pub fn index_of_label(&self, label: &str) -> Option<usize> {
        self.profiles.iter().position(|p| p.label == label)
    }

    pub fn add_custom_from_index(&mut self, base_index: usize) -> StrategyProfile {
        let base = self
            .profiles
            .get(base_index)
            .cloned()
            .unwrap_or_else(|| self.profiles[0].clone());
        let tag = format!("c{:02}", self.next_custom_id);
        self.next_custom_id += 1;
        let fast = base.fast_period.max(2);
        let slow = base.slow_period.max(fast + 1);
        let profile = StrategyProfile {
            label: format!("MA(Custom {}/{}) [{}]", fast, slow, tag),
            source_tag: tag,
            fast_period: fast,
            slow_period: slow,
            min_ticks_between_signals: base.min_ticks_between_signals,
        };
        self.profiles.push(profile.clone());
        profile
    }

    pub fn update_profile(
        &mut self,
        index: usize,
        fast_period: usize,
        slow_period: usize,
        min_ticks_between_signals: u64,
    ) -> Option<StrategyProfile> {
        let profile = self.profiles.get_mut(index)?;
        let fast = fast_period.max(2);
        let slow = slow_period.max(fast + 1);
        profile.fast_period = fast;
        profile.slow_period = slow;
        profile.min_ticks_between_signals = min_ticks_between_signals.max(1);
        if profile.source_tag.starts_with('c') {
            profile.label = format!(
                "MA(Custom {}/{}) [{}]",
                profile.fast_period, profile.slow_period, profile.source_tag
            );
        } else if profile.source_tag == "fst" {
            profile.label = format!("MA(Fast {}/{})", profile.fast_period, profile.slow_period);
        } else if profile.source_tag == "slw" {
            profile.label = format!("MA(Slow {}/{})", profile.fast_period, profile.slow_period);
        } else if profile.source_tag == "cfg" {
            profile.label = format!("MA(Config {}/{})", profile.fast_period, profile.slow_period);
        }
        Some(profile.clone())
    }

}
