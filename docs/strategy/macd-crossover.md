# MACD Crossover (MAC)

## Idea
Use the Moving Average Convergence Divergence momentum shift.

- MACD line: `EMA_fast - EMA_slow`
- Signal line: `EMA_9(MACD)`

## Parameters
- `fast_period`: fast EMA period
- `slow_period`: slow EMA period
- `min_ticks_between_signals`: cooldown

## Entry / Exit
- Buy: MACD crosses above signal line
- Sell: MACD crosses below signal line

## Notes
- Long/flat only.
- Signal EMA period is internally derived from slow period (`clamp(slow/2, 2..9)`).
