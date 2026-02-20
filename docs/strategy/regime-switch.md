# Regime Switch (REG)

## Idea
Switch between two behaviors based on observed volatility regime.

- High volatility: follow trend using fast/slow moving-average crossover.
- Low volatility: use mean reversion around rolling mean and one standard deviation.

## Parameters
- `fast_period`: fast MA period
- `slow_period`: slow MA / volatility window period
- `min_ticks_between_signals`: cooldown in ticks

## Entry / Exit
- Trend regime (`vol_ratio >= threshold`):
  - Buy: fast crosses above slow
  - Sell: fast crosses below slow
- Mean-reversion regime:
  - Buy: `price <= mean - std_dev`
  - Sell: `price >= mean + std_dev`

## Notes
- Long/flat only.
- Regime threshold is internal constant and may be exposed later.
