# AGENTS.md

Guidance for AI agents (and humans) working in this repository.

## What this is

`poker-cli` — a client/server **No-Limit Texas Hold'em** CLI usable by both a
human player and an AI agent. A `serve` process owns the tables and the engine;
every other command is a short-lived client. Planned as a Rust workspace.

## Status

**Design stage — no Rust code yet.** The approved design is the source of truth:

- Design spec: `docs/superpowers/specs/2026-10-05-poker-cli-design.md`
- Agent usage skill: `skills/poker-cli/` (shipped as a Pi package via
  `package.json`; status `draft`, pending validation against the binary)

If code and spec disagree, the spec wins until it is deliberately revised.

## Repository layout

```
docs/superpowers/specs/   Approved design spec(s)
skills/poker-cli/         Agent skill (SKILL.md + references/)
package.json              Pi package manifest (pi.skills + pi-package keyword)
```

Planned workspace crates (not yet scaffolded):

```
poker-core                Engine: rules, betting legality, hand eval, equity, RNG
poker-server              Sessions, tables, tokens, clock, event log, persistence
poker-cli                 Binary: arg parsing, rendering, unix-socket client
```

## Non-negotiable invariants

These are contract, not preference. Do not weaken them without revising the spec.

- **Privacy:** no hole card ever leaves the server except to the seat that owns
  it — including in `feed`, `wait`, and persisted history. The only exception is
  admin `snapshot --reveal`, which is token-gated and tagged `"revealed": true`.
- **Authority split:** the engine is the only poker authority; the CLI does
  shape parsing + rendering only; the server owns tables, tokens, clock, event
  log, and persistence.
- **`legal_actions`** is engine-computed and travels in the snapshot; agents act
  from that menu, never from guesses.
- **One versioned schema, two renderings.** Human output is a rendering of the
  same JSON model; they must not drift.
- **Streams:** data → stdout, errors/diagnostics → stderr. Stable exit codes per
  error class (spec §8.3); JSON failures carry `error.code`.
- **Transport:** unix socket + newline-delimited JSON; the server serializes all
  seat actions (poker turn order is strictly sequential).
- **Flow:** `begin` is always explicit; streets auto-advance in `auto` flow;
  admin `step` is the manual/debug path and bypasses the turn clock.
- **Keep the spec authoritative:** if a change is not in the spec, revise the
  spec first (see the brainstorming skill), then implement.

## Build & test

Not yet scaffolded. Once the workspace exists:

```bash
cargo build
cargo test          # unit + end-to-end (spawns the binary against a temp socket)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Working process

1. Brainstorm and get the design approved (superpowers:brainstorming).
2. Write the spec under `docs/superpowers/specs/`.
3. Create an implementation plan (superpowers:writing-plans) before coding.
4. Implement with TDD (superpowers:test-driven-development); verify before
   claiming done (superpowers:verification-before-completion).
5. Keep the spec and the skill in sync when behavior changes.

Do not write implementation code before the plan exists.

## Distribution

The agent skill ships with the repo as a Pi package:

```bash
pi install git:github.com/<owner>/poker-cli
```
