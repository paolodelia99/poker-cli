# poker-cli reference

Derived from `docs/superpowers/specs/2026-10-05-poker-cli-design.md`. Treat the
spec as authoritative if the two disagree.

## Contents

- Command surface
- `choose` grammar
- Exit codes and error classes
- Session discovery
- JSON shapes (`snapshot`, `status`, `prob`, `event`)
- Table setup options
- Lifecycle and observation

## Command surface

| Command | Semantics |
|---|---|
| `serve [--socket PATH] [--daemon]` | Run the server that owns tables. |
| `start --table ID [options]` | Create a table; caller becomes host (admin token). |
| `tables` | List tables, seats, state, whose turn. |
| `begin [--hands N \| --continuous]` | Host deals the next hand (one hand by default). |
| `end` | Host closes the table (server keeps running). |
| `join [--table ID] [--seat N] [--as NAME] [--watch] [--no-write]` | Take a seat, or `--watch` as a read-only spectator. |
| `leave` | Vacate your seat (folds immediately if mid-hand). |
| `sit-out` / `sit-in` | Pause / resume being dealt in without losing the seat. |
| `snapshot [--reveal]` | Perspective-scoped current state. |
| `status` | Public seats digest (never hole cards). |
| `choose <action> [--if-seq N]` | Act. |
| `prob [--iterations N] [--opponents K] [--seed HEX] [--exact]` | Fair Monte-Carlo equity for your hand. |
| `wait [--turn] [--change] [--until EXPR] [--timeout D] [--since ID]` | Block until a condition holds. |
| `feed [--since ID] [--types LIST] [--timeout D]` | Stream the event log. |
| `flow <auto\|manual>` | Admin: auto-advance streets, or enable `step`. |
| `step` | Admin: advance the engine exactly one transition (manual flow). |
| `replay --hand N [--step \| --auto]` | Admin: re-drive a stored hand. |
| `history [--table ID] [--hand N]` | List or fetch persisted hands. |
| `help [COMMAND]`, `version` | Usage and versions. |

## `choose` grammar

```
poker-cli choose fold
poker-cli choose check
poker-cli choose call
poker-cli choose raise 60          # raise TO 60 total
poker-cli choose raise --by 60     # raise BY 60
poker-cli choose all-in
```

The CLI validates shape only. The engine decides legality; the server returns a
structured error plus fresh `legal_actions` / `turn` when an action is rejected.

## Exit codes and error classes

| Exit | `error.code` | Meaning |
|---|---|---|
| 0 | — | success |
| 1 | `internal` | unexpected bug |
| 2 | `usage` | bad CLI args / unknown command |
| 3 | `no_session` | `.poker` not found / cannot resolve table |
| 4 | `unreachable` | server/socket not reachable |
| 5 | `protocol_version` | client/server version mismatch |
| 6 | `auth` | bad/missing token, or not admin |
| 7 | `not_found` | no such table / seat / hand |
| 10 | `not_your_turn` | acting out of turn |
| 11 | `illegal_action` | action not in `legal_actions` |
| 12 | `stale` | state advanced under the caller |
| 13 | `table_finished` | table ended |
| 14 | `wait_timeout` | bounded `wait` expired without matching |

`illegal_action`, `not_your_turn`, and `stale` include fresh
`legal_actions` / `turn` in the error so a caller can correct itself.

## Session discovery

Resolution order, first match wins:

1. Flags: `--server`, `--table`, `--seat`, `--token`, `--format`.
2. Env: `POKER_SERVER`, `POKER_TABLE`, `POKER_SEAT`, `POKER_TOKEN`,
   `POKER_FORMAT`.
3. Nearest `.poker` file, walking up from the current directory.

`.poker` (permissions `0600`, holds a token):

```json
{
  "v": 1, "role": "player",
  "server": "unix:///run/user/1000/poker/main.sock",
  "table": "hero", "seat": 2, "token": "…", "format": "json"
}
```

`join` with an existing token reattaches to the same seat idempotently.

## JSON shapes

### `snapshot` (schema `poker.snapshot/1`)

```json
{
  "schema": "poker.snapshot/1",
  "seq": 118,
  "revealed": false,
  "table": "hero", "hand": 7, "street": "flop",
  "board": ["Ah", "Kd", "7c"], "pots": {"main": 240, "side": []},
  "button": 3, "blinds": {"sb": 5, "bb": 10, "ante": 0},
  "current_bet": 20, "min_raise_to": 40,
  "turn": {"seat": 2, "deadline_ms": 11840},
  "you": {
    "seat": 2, "name": "alice", "stack": 940, "hole": ["Qs", "Qh"],
    "bet_this_street": 0, "status": "active",
    "legal_actions": {
      "fold": true, "check": false, "call": 20,
      "raise": {"min": 40, "max": 940}, "all_in": 940
    }
  },
  "seats": [
    {"seat": 1, "name": "bob", "stack": 700, "bet": 20,
     "status": "active", "last_action": "raise to 20"}
  ],
  "history": [{"street": "preflop", "seat": 1, "action": "raise to 20"}],
  "events": [{"seq": 117, "type": "action", "seat": 1, "action": "raise to 20"}]
}
```

`legal_actions` always describes the calling seat's current menu. A spectator's
snapshot has no `hole` field anywhere.

### `status` (schema `poker.status/1`)

Public seats only: `seat`, `name`, `stack`, `bet`, `position`, `status`,
`last_action`, plus table/hand/street/turn.

### `prob` (schema `poker.prob/1`)

```json
{"schema": "poker.prob/1", "mode": "monte-carlo", "iterations": 50000,
 "opponents": 2, "equity": {"win": 0.41, "tie": 0.03, "lose": 0.56}}
```

### `event` (schema `poker.event/1`)

```json
{"schema": "poker.event/1", "seq": 118, "type": "action",
 "seat": 1, "street": "preflop", "action": "raise to 20"}
```

Event types: `table.created`, `seat.joined`, `seat.left`, `seat.sat_out`,
`seat.sat_in`, `hand.begin`, `blinds.posted`, `cards.dealt`, `action`,
`street.advanced`, `timeout`, `showdown`, `hand.complete`, `table.finished`.

Every state change bumps the table's monotonic `seq`. Use it for `feed --since`,
`wait --since`, and `choose --if-seq`.

## Table setup options (`start`)

| Option | Meaning | Default |
|---|---|---|
| `--table ID` | table identifier | required |
| `--seats N` | number of seats | 6 |
| `--bots K` | engine-driven bot seats | 0 |
| `--stack S` | starting stack per seat | 1000 |
| `--sb N` / `--bb N` | blinds | 5 / 10 |
| `--ante A` | ante per hand | 0 |
| `--turn-timeout D` | per-turn clock; `0` disables | 60s |
| `--timebank T` | extra time pool per player | 0 |
| `--button SEAT` | initial button seat | 1 |
| `--seed HEX` | RNG seed → reproducible deals | random |
| `--variant` | poker variant | `nlhe` |
| `--flow auto\|manual` | auto-advance or admin-stepped | `auto` |
| `--autostart` | lazily spawn a server if none reachable | off |
| `--no-write` | do not write a `.poker` file | off |

## Lifecycle and observation

- `begin` is always explicit; nothing deals itself. `begin --hands N` or
  `--continuous` continues across hands.
- Auto flow advances streets when betting closes; `flow manual` + `step` is for
  debugging/replay.
- Turn clock (default 60s) applies check-if-free-else-fold on expiry and emits a
  `timeout` event.
- `leave` mid-hand folds you; `sit-out` pauses you safely; one seat left with
  chips auto-ends the table (no rebuys in v1).
- Completed hands persist to `.poker/history/<table>/hand-<n>.json` in the same
  event schema; `history` reads them, `replay` re-drives one.
