# RSA

## Summary
- RSI-based mean-reversion strategy (file name is `rsa`, logic is RSI).

## Category
- mean-reversion

## Signal Math
- Price change:
  $$
  \Delta_t=P_t-P_{t-1}
  $$
- Gain/loss:
  $$
  G_t=\max(\Delta_t,0),\quad L_t=\max(-\Delta_t,0)
  $$
- Wilder smoothing:
  $$
  \overline{G}_t=\frac{(n-1)\overline{G}_{t-1}+G_t}{n},\quad
  \overline{L}_t=\frac{(n-1)\overline{L}_{t-1}+L_t}{n}
  $$
- RSI:
  $$
  RS_t=\frac{\overline{G}_t}{\overline{L}_t},\quad
  RSI_t=100-\frac{100}{1+RS_t}
  $$
- Buy condition:
  $$
  RSI_t\le \text{lower}
  $$
- Sell condition:
  $$
  RSI_t\ge \text{upper}
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `period (n)`: RSI period.
- `lower`: oversold threshold.
- `upper`: overbought threshold.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/rsa.rs`

## Tests
- `tests/rsa_strategy_tests.rs`
