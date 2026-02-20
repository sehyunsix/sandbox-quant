# Opening Range Breakout

## Summary
- Breakout strategy that enters on break of an opening range and exits on trailing range breakdown.

## Category
- breakout

## Signal Math
- Opening range high (oldest `o` samples):
  $$
  H_t=\max(P_{t-o-x},\dots,P_{t-x-1})
  $$
- Trailing range low (latest `x` samples):
  $$
  L_t=\min(P_{t-x},\dots,P_{t-1})
  $$
- Buy condition:
  $$
  P_t>H_t
  $$
- Sell condition:
  $$
  P_t<L_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `opening_window (o)`: opening range width.
- `exit_window (x)`: trailing exit range width.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/opening_range_breakout.rs`

## Tests
- `tests/opening_range_breakout_tests.rs`
