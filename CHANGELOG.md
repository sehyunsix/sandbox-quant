## v0.16.0 - 2026-02-22

- feat(ui): split asset table by spot/futures and add total pnl (382d745)
- fix(ui): map new strategy labels to source tags in strategy table (9b166eb)
- fix(strategy-stats): preload history for all strategy symbols on startup (f2d0c88)

## v0.15.0 - 2026-02-20

- feat(strategy): add MACD ROC Aroon strategies (97ff700)

## v0.14.0 - 2026-02-20

- feat(strategy): add remaining hybrid strategies and catalog wiring (d658de5)

## v0.13.6 - 2026-02-20

- fix: compute futures strategy stats from realized pnl fills (9fa87e1)

## v0.13.5 - 2026-02-20

- docs: linkify strategy status table paths in README (7d1b38a)

## v0.13.4 - 2026-02-20

- docs: add strategy implementation status table (a0ac5b6)

## v0.13.3 - 2026-02-20

- docs: add strategy documentation scaffold and index (1f998a4)

## v0.13.2 - 2026-02-20

- ci: switch gemini workflow to docs-only RFC/issue lane (9c0d315)

## v0.13.1 - 2026-02-20

- ci: tighten strategy scope gate and relax clippy gate (50d88dc)

## v0.13.0 - 2026-02-20

- feat(workflow): enforce strategy_creation mode with structural gates (c4e2a05)

## v0.12.9 - 2026-02-20

- fix(workflow): auto-format and restrict PR paths for gemini runs (3b36b7a)

## v0.12.8 - 2026-02-20

- fix(workflow): force reset to base branch before PR action (9150d63)

## v0.12.7 - 2026-02-20

- fix(workflow): let create-pull-request manage branch lifecycle (3162094)

## v0.12.6 - 2026-02-20

- chore(workflow): add gemini pr workflow and policy docs (a720ed7)

## v0.12.5 - 2026-02-20

- refactor(main): route grid key block via handle_grid_key (b98edde)
- refactor(main): extract grid strategy action handler (c1291e8)
- refactor(main): extract grid selection navigation helpers (5fcb592)
- refactor(main): extract strategy editor command handler (3905193)
- refactor(main): extract popup command handlers from input loop (42702dd)
- refactor(main): add PopupCommand parser for popup key handling (dc54753)
- refactor(main): route grid key handling through GridCommand parser (f8fb709)
- refactor(main): introduce testable UiCommand mapping for base key handling (d11d488)

## v0.12.4 - 2026-02-19

- refactor(main): centralize strategy state sync side-effects (98b8460)
- refactor(main): extract repeated runtime side-effects helpers (531bee7)
- docs(rfc): analyze main runtime orchestration with mermaid (a29e1af)

## v0.12.3 - 2026-02-19

- docs(readme): add English captions for UI screenshots (d341bcf)
- docs(readme): add docs/crates badges with local asset images (fbdb732)
- docs(readme): remove auto ui-docs flow and keep manual screenshot slots (7bdf23a)

## v0.12.2 - 2026-02-19

- docs(rfc): add detailed strategy creation lifecycle and governance (3bf6790)
- docs(rfc): add strategy-expansion-first direction roadmap (914ec77)

## v0.12.1 - 2026-02-19

- docs(rfc): add de-scope and core stabilization plan (3f76633)
- fix(ui): align strategy realized pnl with symbol-scoped asset pnl (57fd809)

## v0.12.0 - 2026-02-18

- feat(ui): add rate-based network SLI metrics and health view (d4c4547)
- docs(rfc): add network monitoring rate and SLI upgrade proposal (a1a1099)

## v0.11.0 - 2026-02-18

- feat(ui): split right panel semantics and stack strategy metrics vertically (454ba2b)
- fix(logging): use real signal symbol and remove ws connected noise (6aa35b5)
- feat(logging): migrate ws/main runtime logs to structured LogRecord events (7d0d097)
- docs(rfc): align logging RFC with OTel/ECS and tracing references (ad215f0)
- feat(asset-table): stream per-symbol pnl updates in real time (b32c145)

## v0.10.0 - 2026-02-18

- fix(pnl): aggregate strategy stats across symbols for grid totals (088cf42)
- fix(network): record fill latency when filled arrives without submitted (3e453c0)
- fix(ws): stop false symbol-change reconnect loop and dedupe connected logs (3da83d9)
- feat(grid): add System Log tab on key 5 (d1cd2e3)

## v0.9.1 - 2026-02-18

- fix(ui): remove remaining v2 grid field references in main loop (8255a90)

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
