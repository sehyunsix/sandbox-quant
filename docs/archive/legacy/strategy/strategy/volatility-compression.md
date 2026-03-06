123# Volatility Compression

## Summary
- Volatility strategy that buys an upside move out of a compressed regime and exits on mean breakdown.

## Category
- volatility

## Signal Math
- Rolling mean:
  $$
  \mu_t=\frac{1}{n}\sum_{i=0}^{n-1}P_{t-i}
  $$
- Rolling standard deviation:
  $$
  \sigma_t=\sqrt{\frac{1}{n}\sum_{i=0}^{n-1}(P_{t-i}-\mu_t)^2}
  $$
- Relative band width:
  $$
  BW_t=\frac{2\sigma_t}{|\mu_t|}
  $$
- Compression threshold:
  $$
  \theta=\frac{\text{thresholdBps}}{10000}
  $$
- Buy condition:
  $$
  BW_t\le\theta \land P_t>\mu_t+\sigma_t
  $$
- Sell condition:
  $$
  P_t<\mu_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `period (n)`: rolling window size.
- `threshold_bps`: compression threshold in basis points.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/volatility_compression.rs`

## Tests
- `tests/volatility_compression_tests.rs`
