# Stochastic Reversion

## Summary
- Mean-reversion strategy using stochastic %K oversold/overbought thresholds.

## Category
- mean-reversion

## Signal Math
- Window extrema:
  $$
  H_t=\max(P_{t-n+1},\dots,P_t),\quad
  L_t=\min(P_{t-n+1},\dots,P_t)
  $$
- Stochastic %K:
  $$
  \%K_t = 100\cdot\frac{P_t-L_t}{\max(H_t-L_t,\epsilon)}
  $$
- Thresholds:
  $$
  \text{upper}=u,\quad \text{lower}=100-u
  $$
- Buy condition:
  $$
  \%K_t\le \text{lower}
  $$
- Sell condition:
  $$
  \%K_t\ge \text{upper}
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `lookback (n)`: stochastic lookback window.
- `upper_threshold (u)`: overbought threshold.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/stochastic_reversion.rs`

## Tests
- `tests/stochastic_reversion_tests.rs`
