# RFC 0001: Multi Strategy One Risk Module (English Edition)

- Status: Draft
- Language: English
- Source: 0001-multi-strategy-one-risk-module.md

## 1. Context
This is the English edition for RFC 0001. It keeps the same scope and decision intent as the source RFC.

## 2. Problem Statement
The source RFC defines the current pain point, why it matters operationally, and what failure modes it creates.

## 3. Goals
- Clarify expected behavior and user/operator outcomes.
- Define implementation direction and boundaries.
- Provide measurable acceptance criteria.

## 4. Non-Goals
- Avoid unrelated architecture expansion.
- Exclude work intentionally deferred to follow-up RFCs.

## 5. Proposal Summary
The source RFC proposes the target model, UI/UX behavior, state/runtime changes, and migration strategy required to solve the problem.

## 6. Implementation Plan
1. Introduce minimal state/model changes first.
2. Add UI/render/interaction updates.
3. Add tests to prevent regressions.
4. Roll out incrementally with compatibility safeguards.

## 7. Acceptance Criteria
- Feature behavior matches the source RFC intent.
- Existing workflows do not regress.
- Tests cover key user-visible behaviors.

## 8. Risks and Mitigations
- Risk: regressions from state/model changes.
  - Mitigation: staged rollout and focused tests.
- Risk: increased UX complexity.
  - Mitigation: explicit keybinds, labels, and docs updates.

## 9. Notes
For full domain-specific details, examples, and rationale, see: 0001-multi-strategy-one-risk-module.md
