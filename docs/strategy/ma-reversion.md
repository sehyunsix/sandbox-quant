# MA Reversion

## Summary
- Mean-reversion strategy that buys discount to SMA and exits on mean recovery.

## Category
- mean-reversion

## Signal Math
- Mean:
  $$
  \mu_t=\frac{1}{n}\sum_{i=0}^{n-1} P_{t-i}
  $$
- Entry line (discount):
  $$
  B_t=\mu_t(1-\theta),\quad \theta=\frac{\text{thresholdBps}}{10000}
  $$
- Buy condition:
  $$
  P_t\le B_t
  $$
- Sell condition:
  $$
  P_t\ge \mu_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `period (n)`: SMA window.
- `threshold_bps`: discount threshold in basis points.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/ma_reversion.rs`

## Tests
- `tests/ma_reversion_tests.rs`
