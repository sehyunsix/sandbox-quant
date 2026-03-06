# Donchian Trend

## Summary
- Trend-following breakout strategy using Donchian entry/exit channels.

## Category
- trend-following

## Signal Math
- Donchian upper (entry):
  $$
  U_t=\max(P_{t-e},\dots,P_{t-1})
  $$
- Donchian lower (exit):
  $$
  D_t=\min(P_{t-x},\dots,P_{t-1})
  $$
- Buy condition:
  $$
  P_t>U_t
  $$
- Sell condition:
  $$
  P_t<D_t
  $$
- Cooldown gate:
  $$
  (t-\text{lastSignalTick})\ge \text{minTicksBetweenSignals}
  $$

## Parameters
- `entry_window (e)`: breakout lookback.
- `exit_window (x)`: protective exit lookback.
- `min_ticks_between_signals`: debounce/cooldown.

## Source
- `src/strategy/donchian_trend.rs`

## Tests
- `tests/donchian_trend_tests.rs`
