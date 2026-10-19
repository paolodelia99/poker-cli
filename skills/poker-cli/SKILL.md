---
name: poker-cli
description: Use when playing or observing Texas Hold'em through the poker-cli tool - starting or joining a table, waiting for your turn, reading snapshot legal_actions, choosing fold/check/call/raise/all-in, spectating, or handling poker-cli JSON output and exit codes. Triggers: poker-cli, hold'em, poker table, "wait --turn", legal_actions, "choose raise", snapshot, equity, feed.
metadata:
  status: draft
  note: Written from the CLI design spec (docs/superpowers/specs/2026-10-05-poker-cli-design.md). Validate against the implemented binary before relying on exact flags.
---

# Driving poker-cli

`poker-cli` is a client/server Texas Hold'em CLI: a `serve` process owns the
table; every other command is a short-lived client. It is built for agents:
engine-computed **`legal_actions`**, a blocking **`wait`**, versioned JSON, and
stable exit codes.

## Bootstrap (first, every session)

- **Pin machine output:** `export POKER_FORMAT=json`. Without it, output is
  human text when stdout is a TTY, JSON only when piped.
- **Let the CLI find the session**: explicit flags > env
  (`POKER_SERVER` / `POKER_TABLE` / `POKER_SEAT`) > nearest `.poker` file. Never
  hardcode socket paths.
- **Data is on stdout, errors on stderr.**

## Canonical agent loop

```bash
poker-cli wait --turn              # block until my seat is on turn (don't poll)
poker-cli snapshot                 # read you.legal_actions and seq
poker-cli choose raise --by 60     # pick exactly one legal action
```

Never busy-loop `snapshot`. `wait --change` awaits any change to your seat;
`feed` streams the whole table.

## Hard rules

- **`legal_actions` is the menu.** Act only from it. `raise <to>` means "raise to
  that total"; `raise --by N` means "by N". Never guess an amount.
- **Never act out of turn.** `not_your_turn` (exit 10) means wait again.
- **Never read another seat's hole cards.** They belong to their owner only.
  `--reveal` requires an admin token and is not for players or spectators.
- **Branch on exit codes / `error.code`, not on prose.** Correctable failures
  return fresh `legal_actions` / `turn`, so self-heal in one step.
- **`stale` (exit 12)** means state moved under you: re-snapshot. Pass
  `--if-seq <seq>` to act only if nothing changed.
- **`wait --timeout` expiry is exit 14 (`wait_timeout`)** — expected, not a bug;
  just `wait` again.

## Quick reference

| Goal | Command |
|---|---|
| Create a table (host) | `poker-cli start --table hero --seats 2 --stack 1000 --sb 5 --bb 10` |
| Take a seat | `poker-cli join --table hero --seat 2 --as agent` |
| Spectate (read-only) | `poker-cli join --table hero --watch` |
| Start a hand (host) | `poker-cli begin` |
| Current state | `poker-cli snapshot` |
| Public seats | `poker-cli status` |
| Equity | `poker-cli equity --iterations 50000 --opponents 1` |
| Watch live | `poker-cli feed --since <seq>` |
| Leave / pause | `poker-cli leave` / `poker-cli sit-out` |

## Common mistakes

- Parsing human table output instead of setting `POKER_FORMAT=json`.
- Polling `snapshot` instead of using `wait --turn`.
- Inventing a raise amount outside `legal_actions.raise`.
- Waiting without `--timeout` when the opponent may have disconnected.

Full command list, exit codes, and JSON shapes: `references/poker-cli-reference.md`.
