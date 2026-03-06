# Multi-Broker Demo Integration Proposal (Stocks / Options)

This repo currently focuses on Binance demo trading. To make stock/options venue expansion practical without risking live capital, this change adds a dedicated probe tool for paper/sandbox environments:

- Alpaca Paper (`https://paper-api.alpaca.markets`) for paper stock/options workflows
- Tradier Sandbox (`https://sandbox.tradier.com/v1`) for paper stock/options workflows

## Why this feature first

Before routing strategy orders to a new venue, we need a repeatable way to validate credentials, network access, and account reachability in each paper environment. The probe binary provides that minimum integration contract.

## Included in this PR

- `src/bin/demo_broker_probe.rs`
  - Verifies Alpaca paper account reachability via `GET /v2/account`
  - Verifies Tradier sandbox profile reachability via `GET /user/profile`
  - Emits `OK | SKIPPED | FAILED` per broker

## Next step roadmap

1. Introduce a broker trait (`place_market_order`, `get_positions`, `get_order_status`) and map it to Binance/Alpaca/Tradier adapters.
2. Add symbol normalization for equities and OCC option symbology.
3. Add paper-order E2E integration tests using dedicated test accounts.
4. Add broker capability flags (`supports_options`, `supports_fractional`, `delayed_data`).
