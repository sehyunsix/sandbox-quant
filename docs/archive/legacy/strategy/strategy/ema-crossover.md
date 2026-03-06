# EMA Crossover

## Summary
- Exponential moving-average crossover variant for faster trend response.

## Category
- trend-following

## Signal Math
- EMA recurrence:
  $$
  \text{EMA}_t=\alpha P_t+(1-\alpha)\text{EMA}_{t-1},\quad \alpha=\frac{2}{n+1}
  $$
- Fast/slow EMA:
  $$
  \text{EMA}^{(f)}_t,\ \text{EMA}^{(s)}_t,\quad f<s
  $$
- Buy condition:
  $$
  \text{prevFast}\le \text{prevSlow} \land \text{fast}>\text{slow}
  $$
- Sell condition:
  $$
  \text{prevFast}\ge \text{prevSlow} \land \text{fast}<\text{slow}
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `fast_period (f)`: fast EMA window.
- `slow_period (s)`: slow EMA window.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/ema_crossover.rs`

## Tests
- `tests/ema_crossover_tests.rs`
