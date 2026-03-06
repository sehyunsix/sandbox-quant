# Channel Breakout

## Summary
- Breakout strategy using channel highs/lows as trigger boundaries.

## Category
- breakout

## Signal Math
- Entry channel high:
  $$
  H^{(e)}_t=\max(P_{t-e},\dots,P_{t-1})
  $$
- Exit channel low:
  $$
  L^{(x)}_t=\min(P_{t-x},\dots,P_{t-1})
  $$
- Buy condition:
  $$
  P_t>H^{(e)}_t
  $$
- Sell condition:
  $$
  P_t<L^{(x)}_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `entry_window (e)`: breakout lookback.
- `exit_window (x)`: trailing stop lookback.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/channel_breakout.rs`

## Tests
- `tests/channel_breakout_tests.rs`
