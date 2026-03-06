# Aroon Trend (ARN)

## Idea
Track recency of highs/lows in a rolling window to estimate trend strength.

- Aroon Up: how recently a window high occurred
- Aroon Down: how recently a window low occurred

## Parameters
- `fast_period`: Aroon period
- `slow_period`: threshold (e.g., 70)
- `min_ticks_between_signals`: cooldown

## Entry / Exit
- Buy: `AroonUp >= threshold` and `AroonDown <= 100 - threshold`
- Sell: `AroonDown >= threshold` and `AroonUp <= 100 - threshold`

## Notes
- Long/flat only.
- Implementation uses rolling tick-price highs/lows.
