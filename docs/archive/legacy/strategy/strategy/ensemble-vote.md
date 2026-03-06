# Ensemble Vote (ENS)

## Idea
Aggregate multiple lightweight voters and emit signal only when majority aligns.

Current voters:
- MA crossover direction vote
- RSI overbought/oversold vote
- Price vs fast mean vote

## Parameters
- `fast_period`: fast MA period and RSI period seed
- `slow_period`: slow MA period
- `min_ticks_between_signals`: cooldown in ticks

## Entry / Exit
- Buy: vote score `>= +2`
- Sell: vote score `<= -2`

## Notes
- Long/flat only.
- Majority voting reduces single-indicator noise.
