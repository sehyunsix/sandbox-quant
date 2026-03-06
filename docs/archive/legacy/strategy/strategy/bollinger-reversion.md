# Bollinger Reversion

## Summary
- Mean-reversion strategy using Bollinger lower-band entry and mean-recovery exit.

## Category
- mean-reversion

## Signal Math
- Rolling mean:
  $$
  \mu_t=\frac{1}{n}\sum_{i=0}^{n-1}P_{t-i}
  $$
- Rolling standard deviation:
  $$
  \sigma_t=\sqrt{\frac{1}{n}\sum_{i=0}^{n-1}(P_{t-i}-\mu_t)^2}
  $$
- Lower band:
  $$
  L_t=\mu_t-k\sigma_t
  $$
  where `k = band_mult_x100 / 100`.
- Buy condition:
  $$
  P_t\le L_t
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
- `period (n)`: Bollinger window.
- `band_mult_x100`: band multiplier in x100 scale.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/bollinger_reversion.rs`

## Tests
- `tests/bollinger_reversion_tests.rs`
