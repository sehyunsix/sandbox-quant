# GEMINI.md

This document defines guardrails for Gemini-assisted RFC/issue proposal PR generation in this repository.

## Purpose
- Keep AI-generated changes safe, reviewable, and scoped.
- Restrict automation to proposal artifacts (RFC/issue docs), not runtime strategy code generation.

## Trigger Modes
- `workflow_dispatch` in `.github/workflows/gemini-pr.yml`
- Issue comment command (authorized users only):
  - `/gemini-rfc scope=<short task>`
  - `/gemini-issue scope=<short task>`

## Required Validation Gates
Before a Gemini PR is mergeable, all must pass:
- docs-only scope gate
- task-type structural gate (`docs/rfcs` for RFC, `docs/issues` for issue triage)

## Scope Discipline
- Keep each Gemini PR single-purpose and small.
- Do not modify runtime files (`src/**`, `tests/**`, `Cargo.toml`, `Cargo.lock`) from Gemini automation lane.
- Keep workflow-infra updates separate from proposal content when possible.

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

Note:
- Strategy implementation/registration/testing is manual lane only for now.

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
- Use Gemini automation lane to generate production strategy code.

## Review Policy
- Every Gemini PR requires human review.
- If core files are touched (`src/main.rs`, `src/order_manager.rs`, `src/risk_module.rs`, `src/strategy_catalog.rs`), require additional maintainer approval.
