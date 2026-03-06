# Strategy Docs

This directory documents each strategy implementation under `src/strategy/`.

## Goals
- Keep strategy intent and behavior explicit.
- Make review and onboarding faster.
- Track parameters, risks, and test coverage per strategy.

## Strategy Index
- `ma-crossover.md`
- `ema-crossover.md`
- `atr-expansion.md`
- `channel-breakout.md`
- `rsa.md`
- `donchian-trend.md`
- `ma-reversion.md`
- `bollinger-reversion.md`
- `stochastic-reversion.md`
- `volatility-compression.md`
- `opening-range-breakout.md`
- `regime-switch.md`
- `ensemble-vote.md`
- `macd-crossover.md`
- `roc-momentum.md`
- `aroon-trend.md`

## Authoring Rule
- Use `docs/strategy/_template.md` when adding a new strategy.
- When adding `src/strategy/<name>.rs`, add `docs/strategy/<name>.md` in the same PR.
