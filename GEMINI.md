# GEMINI.md

This document defines guardrails for Gemini-assisted changes and PR generation in this repository.

## Purpose
- Keep AI-generated changes safe, reviewable, and scoped.
- Prevent Gemini workflow mechanics from overlapping strategy-domain governance.

## Trigger Modes
- `workflow_dispatch` in `.github/workflows/gemini-pr.yml`
- Issue comment command (authorized users only):
  - `/gemini-pr scope=<short task>`

## Required Safety Gates
Before a Gemini PR is mergeable, all must pass:
- `cargo fmt --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test -q`

## Scope Discipline
- Keep each Gemini PR single-purpose and small.
- Do not mix workflow-infra changes and large strategy-domain refactors in one PR.
- If both are needed, split into two PRs:
  - PR A: workflow/mechanics (`workflow:gemini`)
  - PR B: strategy logic (`domain:strategy`)

## Attribution Rules
For AI-assisted PRs, include:
- `Generated-by: Gemini`
- `Workflow: local-lane | automation-lane`
- `Prompt-Scope: ...`
- `Human-Reviewer: @...`

Recommended commit trailers:
- `AI-Generated: Gemini`
- `Co-authored-by: Gemini <gemini-noreply@google.com>`

## Runtime Isolation Rules (Strategy Safety)
- A failing strategy must not stop global runtime.
- Strategy failures are isolated per strategy id/source tag.
- Repeated failures move strategy to quarantined state.
- Quarantined strategy requires explicit human action to re-enable.

## Non-Negotiable Do/Do Not
Do:
- Add/update tests in `tests/` for behavior changes.
- Keep diffs minimal and reversible.
- Document migration notes when public APIs or persisted schema change.

Do Not:
- Auto-merge Gemini PRs to `main`.
- Bypass required checks.
- Introduce destructive git operations in automation.
- Expand scope beyond requested task without opening follow-up PR.

## Review Policy
- Every Gemini PR requires human review.
- If core files are touched (`src/main.rs`, `src/order_manager.rs`, `src/risk_module.rs`, `src/strategy_catalog.rs`), require additional maintainer approval.
