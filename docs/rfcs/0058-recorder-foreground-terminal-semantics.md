# RFC 0058: Recorder Foreground Terminal Semantics

## Status
Draft

## Summary
`sandbox-quant-recorder` should stop behaving like a thin process manager in interactive mode.

The recorder binary already owns its own lifecycle. Keeping shell-like commands while still trying to supervise a separate recorder process introduces duplicated ownership, lock conflicts, stale status files, and hard-to-debug crashes.

This RFC sets the interactive semantics for the recorder terminal:

- interactive `sandbox-quant-recorder` is the recorder process
- `/start` starts recording in the current process
- `/status` reads in-process recorder state
- `/stop` stops the in-process recorder
- no background process is spawned from the interactive shell

One-shot control commands and background supervision remain possible in separate code paths, but interactive mode must be foreground and self-owned.

## Problem

The previous design mixed two incompatible models:

1. Separate recorder binary with its own lifecycle
2. Interactive shell that tried to supervise or inspect another recorder process via status/config files

This caused:

- process ownership ambiguity
- stale status files being mistaken for live state
- database lock conflicts
- repeated crashes or confusing behavior from `/start` and `/status`

## Decision

Interactive `sandbox-quant-recorder` uses foreground ownership.

Meaning:

- `sandbox-quant-recorder` with no subcommand enters a line-based terminal
- that terminal owns a single in-process `MarketDataRecorder`
- `/start [symbols...]` mutates or starts that in-process recorder
- `/status` renders the current in-memory recorder state
- `/stop` stops the current in-process recorder

The terminal must not shell out to, spawn, or supervise another recorder process.

## Terminal Mode

Recorder and backtest terminals should use a plain line-oriented REPL mode instead of raw cursor-driven terminal control.

Reasons:

- lower crash risk
- fewer TTY edge cases
- more stable behavior under `cargo run`, pipes, and pseudo-TTY environments
- sufficient for non-operator terminals

Operator terminal may continue using the richer raw terminal experience.

## Ownership Rules

- `sandbox-quant` never manages recorder lifecycle
- `sandbox-quant-recorder` interactive mode never manages another recorder process
- status/config files are for explicit lifecycle control paths only
- in-process recorder state is the source of truth during an interactive recorder session

## Consequences

Positive:

- less ambiguous lifecycle
- fewer lock conflicts
- easier debugging
- recorder shell behavior matches user expectations

Trade-offs:

- interactive recorder no longer acts like a daemon manager
- detached/background management must be treated as a separate path and kept clearly separate from interactive mode

## Follow-up

- simplify or remove process-manager code that is no longer required for interactive recorder flows
- expose health and ingestion diagnostics directly from in-process recorder state
- consider applying the same semantic simplification to backtest interactive mode
