# ATR Expansion

## Summary
- Volatility expansion strategy based on ATR dynamics.

## Category
- volatility

## Signal Math
- Price delta:
  $$
  \Delta_t=P_t-P_{t-1}
  $$
- ATR proxy (EMA of absolute delta):
  $$
  \text{ATR}_t=\text{EMA}_n(|\Delta_t|)
  $$
- Threshold:
  $$
  \theta_t=\text{ATR}_t \cdot m
  $$
  where `m = threshold_x100 / 100`.
- Buy condition:
  $$
  \Delta_t>\theta_t
  $$
- Sell condition:
  $$
  \Delta_t<-\theta_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `period (n)`: ATR EMA period.
- `threshold_x100`: expansion multiplier in x100 scale.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/atr_expansion.rs`

## Tests
- `tests/atr_expansion_tests.rs`
