## v0.9.0 - 2026-02-18

- feat(ui): add portfolio grid 3-tab layout and risk-focused views (c0c8894)

## v0.8.1 - 2026-02-18

- docs(readme): add docs.rs link (74c3a05)

## v0.8.0 - 2026-02-18

- feat(ui): improve portfolio grid paneling and asset aggregation (98a9c08)
- feat(runtime): enable concurrent multi-symbol strategy execution (379b8b7)
- docs(rfc): define concurrent multi-strategy enable plan (ce6e621)

## v0.7.0 - 2026-02-18

- docs(rfc): propose strategy lifecycle metrics display (983562b)
- fix(ui): allow quitting from any screen with q (afe53e4)
- fix(strategy): keep symbol scoped per strategy profile (345408c)
- feat(strategy): enforce fork-on-edit for config changes (40eb250)

## v0.6.0 - 2026-02-18

- docs(rfc): require fork-on-edit for strategy config changes (643b7a0)
- fix(ui): enlarge strategy table area and columns (90f5b3c)
- refactor(ui): render strategy table with ratatui Table (667fb21)
- fix(ui): show symbol in strategy views (ad9fa18)
- chore(ui): improve strategy table readability (1db7600)
- fix(ui): allow symbol selection in strategy config editor (f82b260)
- feat(ui): show symbol in strategy grid table (48cf506)
- fix: persist strategy session across restarts (83f2587)
- feat(ui): support strategy create and config edit from grid (5870d26)

## v0.5.0 - 2026-02-17

- feat(ui): select strategy from grid and jump to focus (a259e47)
- test(ui): add focus drill-down render and fallback focus tests (cb2d7c8)

## v0.4.0 - 2026-02-17

- feat(ui): add focus drill-down popup with state persistence (f452396)
- feat(ui): add V2 grid popup with risk-rate heatmap and rejection stream (6bf2653)
- feat(ui): add AppStateV2 scaffold with legacy mapping tests (1bb8de2)

## v0.3.0 - 2026-02-17

- release correction: normalize accidental v1.0.0 bump to v0.3.0
- fix(ci): remove feat(break) from automatic major bump triggers

## v1.0.0 - 2026-02-17

- feat(break): trigger 0.3.0 release (7c08184)

## v0.2.18 - 2026-02-17

- fix(ci): treat feat(break) as major bump trigger (f8b1080)

## v0.2.17 - 2026-02-17

- fix(ci): treat feat commits as minor version bump (980a971)

## v0.2.16 - 2026-02-17

- test(risk): verify rejection when global or symbol limits are exceeded (6d0cc37)
- test(runtime): add concurrent limit and 10x3 dispatch validations (095a1ea)
- test(risk): add endpoint/global budget throttling coverage (e779c0a)
- feat(runtime): make worker dispatch deterministic and move tests to tests/ (0290ad2)

## v0.2.15 - 2026-02-17

- docs(agent): require tests in tests/ and for every feature (ac96c16)

## v0.2.14 - 2026-02-17

- test(order_store): add strategy stats persistence tests with docs (a3488cd)
- feat(runtime): queue strategy/manual signals through shared risk channel (e21eb78)
- feat(runtime): add strategy worker registry and symbol tick channels (5388d46)
- feat(stats): persist strategy+symbol stats for restart recovery (cfc287c)
- feat(risk): add symbol exposure and endpoint budget guardrails (6625e93)
- feat(risk): enforce per-symbol exposure limits (65db24a)

## v0.2.13 - 2026-02-17

- fix(ci): run crate publish in main release workflow (7a67457)

## v0.2.12 - 2026-02-17

- feat(risk): add per-strategy cooldown and active-order limits (8fb5da3)

## v0.2.11 - 2026-02-17

- chore(ci): harden cargo publish workflow (059f2a8)

## v0.2.10 - 2026-02-17

- chore: adopt MIT license (b88e81b)

## v0.2.9 - 2026-02-17

- docs: expand risk/order manager docstrings with usage and cautions (1dae5af)

## v0.2.8 - 2026-02-17

- docs: add browser docs portal and enrich risk/order docstrings (73f069f)

## v0.2.7 - 2026-02-17

- refactor(order): remove duplicated risk helpers after module split (95343a0)

## v0.2.6 - 2026-02-17

- refactor(risk): extract risk module from order manager (6ac11df)

## v0.2.5 - 2026-02-17

- ci(release): auto-update changelog history on version bump (703093f)

## v0.2.2 - 2026-02-17

- chore(release): v0.2.2
- feat(risk): surface intent id end-to-end and add auto version release workflow

## v0.2.1 - 2026-02-17

- chore(release): v0.2.1
- release: bump to 0.2.1 with risk config and reason code standardization
- feat(risk): introduce order intent evaluation and rejection reason codes
