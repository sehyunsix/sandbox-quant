# RFC 0025: Gemini Workflow for Branch-to-PR Automation

## Summary
- Define a practical workflow that uses Gemini to generate changes and open PRs with repeatable quality gates.
- Separate synchronous local coding from asynchronous GitHub automation.
- Keep human control on merge decisions while reducing repetitive PR preparation work.

## Why This RFC
- Current flow is fast for one-off edits, but repetitive for large-scale strategy expansion.
- We need a pipeline that can repeatedly:
  - pick scoped tasks
  - implement changes
  - run checks
  - open PRs with structured context
- Goal: higher throughput without lowering review quality.

## External Reference Direction
- Official Gemini CLI Action exists for GitHub workflows and supports `workflow_dispatch`/scheduled usage.
- Gemini Code Assist GitHub App supports repository-level AI interactions from GitHub.
- GitHub Actions security guidance recommends least-privilege permissions and protected environments.

## Decision
- Adopt a **two-lane model**:
  1. **Local Lane (fast iteration)**: developer/agent runs Gemini locally for immediate changes.
  2. **Automation Lane (async PR pipeline)**: GitHub Action runs Gemini on queued scopes and prepares PRs.

## Boundary: Avoid Overlap With Strategy Workflow
- Problem:
  - Gemini automation flow and strategy development flow can collide when both try to drive the same lifecycle (idea -> implementation -> promotion).
- Policy:
  - Gemini workflow owns **change production mechanics**:
    - branch creation
    - patch generation
    - test execution
    - PR drafting
  - Strategy workflow owns **domain decisions**:
    - strategy taxonomy and acceptance criteria
    - risk gates and promotion decisions
    - live enable/disable policy
- Required split:
  - `docs/rfcs/0024-*` remains the source of truth for strategy roadmap and quality gates.
  - `docs/rfcs/0025-*` governs only AI PR automation mechanics.
- Enforcement:
  - PR label contract:
    - `workflow:gemini` for automation mechanics changes
    - `domain:strategy` for strategy logic changes
    - both labels may exist, but reviewers must approve in both domains.
  - CODEOWNERS split:
    - workflow files require platform maintainer review
    - strategy engine/catalog files require quant maintainer review

## Proposed Architecture
### Lane A: Local Lane
- Use short commands (`c`, `cp`, `cpp`, `cppm`) for immediate branch-based delivery.
- Best for:
  - focused bug fixes
  - interactive UI changes
  - tasks requiring manual validation in local runtime

### Lane B: Automation Lane
- Trigger modes:
  - `workflow_dispatch` with explicit task input
  - `issue_comment` command trigger (e.g. `/gemini-pr scope=...`)
  - optional schedule for issue triage only
- Worker job steps:
  1. create working branch from `main`
  2. run Gemini prompt template for scoped task
  3. run `cargo test -q` and lint/format gates
  4. open PR as draft with checklist + risk notes
- No auto-merge in initial rollout.

## PR Contract (Required)
- Every AI-generated PR must include:
  - scope statement
  - impacted modules/files
  - test evidence (`cargo test -q` result summary)
  - rollback notes
  - AI contribution attribution section
- Label set:
  - `ai-generated`
  - `needs-human-review`
  - `strategy-expansion` (when relevant)

## Contributor Attribution (Gemini)
- Why:
  - AI contribution traceability is required for governance and auditability.
- Required metadata in AI-generated PR body:
  - `Generated-by: Gemini`
  - `Workflow: local-lane | automation-lane`
  - `Prompt-Scope: <short scope>`
  - `Human-Reviewer: <github-handle>`
- Recommended commit trailer format:
  - `Co-authored-by: Gemini <gemini-noreply@google.com>`
  - `AI-Generated: Gemini`
- Optional repository-level recognition:
  - Add a `Contributors` section in README with:
    - `Gemini (AI-assisted contributions via reviewed PRs)`
  - Keep human contributors listed separately to avoid ambiguity.

## Security and Permissions
- Use minimum Action permissions:
  - `contents: write` (branch push)
  - `pull-requests: write` (PR creation)
  - avoid broad repository admin scopes
- Store Gemini key in repository/environment secrets.
- Keep branch protection and required checks enabled on `main`.

## Failure and Recovery Model
- If generation fails checks:
  - keep branch
  - open issue/comment with failure summary and failing command
- If PR is stale/conflicted:
  - close with reason and re-queue task
- If repeated failures exceed threshold:
  - auto-downgrade task to manual lane

## Runtime Safety: Isolate Gemini Strategy Failures
- Goal:
  - A failing Gemini-generated strategy must not break core runtime or local-maintained strategies.
- Isolation policy:
  - Execute strategies with **per-strategy fault domains** (logical isolation by `source_tag` / strategy id).
  - On strategy failure, disable only the failing strategy instance, never the global loop.
- Required guards:
  1. **Per-strategy circuit breaker**
     - N consecutive runtime errors -> auto OFF for that strategy.
     - Keep other strategies ON.
  2. **Panic boundary**
     - Wrap strategy tick evaluation with panic-safe boundary (`catch_unwind`) and convert panic to strategy-local error event.
  3. **Fail-open runtime**
     - Global event loop continues even when one strategy errors.
  4. **Quarantine state**
     - Mark failed strategy as `quarantined` and block re-enable until explicit operator action.
  5. **Canary-first promotion**
     - New Gemini strategies run in shadow/canary mode first (no order submit or capped exposure), then graduate.
  6. **Per-strategy risk ceiling**
     - Strictly lower default limits for Gemini-origin strategies until promoted.
- Operational visibility:
  - Emit structured event:
    - `strategy.runtime.error`
    - `strategy.runtime.panic`
    - `strategy.quarantined`
    - `strategy.recovered`
  - Show quarantine status in portfolio grid strategy table.
- Recovery path:
  - Operator can:
    - keep quarantined
    - reset error counter and re-enable
    - delete strategy profile
  - All actions are audit-logged.

## Separation of Trust Levels
- Introduce strategy provenance:
  - `origin=local` (human-maintained)
  - `origin=gemini` (AI-generated)
- Policy by origin:
  - `origin=gemini`:
    - stricter cooldown/exposure defaults
    - mandatory canary phase
    - quarantine on repeated faults
  - `origin=local`:
    - normal production policy
- This keeps Gemini failures from degrading trusted local strategy operation.

## Compile-time Safety: Block Broken PRs Before Merge
- Problem:
  - Even if runtime isolation exists, merged code can still break build/test at compile stage.
- Policy:
  - No Gemini-generated PR is mergeable unless compile-time gates are green.
- Required compile-time gates (mandatory):
  1. `cargo check --all-targets`
  2. `cargo test -q`
  3. `cargo clippy --all-targets -- -D warnings` (or project-agreed warning policy)
  4. `cargo fmt --check`
- Merge protection:
  - Mark the above checks as required status checks in branch protection.
  - Disallow bypass except for repository admins in emergency mode.
- Compatibility checks:
  - Add matrix build for:
    - stable toolchain
    - minimum supported Rust version (MSRV, if defined)
  - This prevents “works on one machine” compile regressions.
- Scope-change guard:
  - If PR touches core runtime files (`src/main.rs`, `src/order_manager.rs`, `src/risk_module.rs`, `src/strategy_catalog.rs`), require an additional maintainer approval.
- Generated-code confidence guard:
  - For Gemini PRs, enforce “no unchecked API changes”:
    - if public structs/enums/functions changed, require changelog note and migration note in PR body.
- Rollback-ready merge:
  - Every merged Gemini PR must be revertable as a single commit or clean commit stack.
  - This minimizes recovery time if latent compile/runtime issues surface later.

## Rollout Plan
1. **Phase 1: RFC + templates**
   - Prompt template, PR template, labels, checklist.
2. **Phase 2: Dispatch-only workflow**
   - Manual trigger from GitHub Actions UI.
3. **Phase 3: Comment trigger**
   - Controlled `/gemini-pr` command for maintainers.
4. **Phase 4: Limited automation expansion**
   - Strategy expansion batches with strict gate policy.

## Non-Goals
- Fully autonomous merge to `main`.
- Unbounded issue-to-code automation without human triage.
- Replacing local interactive development flow.

## Success Criteria
- PR preparation time reduced for repetitive tasks.
- No reduction in test pass rate or review quality.
- AI-generated PRs remain auditable and rollback-friendly.
- All AI-generated PRs include explicit Gemini attribution metadata.
- Gemini strategy failures are contained to strategy-local scope without stopping global runtime.
- Gemini-generated PRs do not degrade compile/test baseline after merge.

## References
- Gemini CLI GitHub Action: https://github.com/google-github-actions/run-gemini-cli
- Gemini Code Assist for GitHub: https://marketplace.visualstudio.com/items?itemName=Google.geminicodeassist
- GitHub Actions security hardening: https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions
