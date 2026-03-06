# ROC Momentum (ROC)

## Idea
Use Rate of Change (ROC) to detect directional momentum bursts.

- `ROC = (P_t - P_{t-n}) / P_{t-n}`

## Parameters
- `fast_period`: lookback period `n`
- `slow_period`: ROC threshold in basis points
- `min_ticks_between_signals`: cooldown

## Entry / Exit
- Buy: `ROC >= +threshold`
- Sell: `ROC <= -threshold`

## Notes
- Long/flat only.
- Useful as a simple momentum engine or ensemble component.
