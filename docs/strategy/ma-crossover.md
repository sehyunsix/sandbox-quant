# MA Crossover

## Summary
- Moving-average crossover strategy using configurable fast/slow windows.

## Category
- trend-following

## Signal Math
- Fast SMA:
  $$
  \text{SMA}^{(f)}_t=\frac{1}{f}\sum_{i=0}^{f-1} P_{t-i}
  $$
- Slow SMA:
  $$
  \text{SMA}^{(s)}_t=\frac{1}{s}\sum_{i=0}^{s-1} P_{t-i},\quad f<s
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
- `fast_period (f)`: fast SMA window.
- `slow_period (s)`: slow SMA window.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/ma_crossover.rs`

## Tests
- `tests/ma_crossover_tests.rs`
