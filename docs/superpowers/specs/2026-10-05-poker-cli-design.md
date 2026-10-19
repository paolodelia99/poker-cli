# Poker CLI — Design Specification

- **Status:** Approved (brainstorming complete)
- **Date:** 2026-10-05
- **Scope:** The `poker-cli` command-line product: its process model, protocol,
  command surface, view model, error contract, and persistence.
- **Out of scope (separate spec):** the game engine and poker rules
  (`poker-core`), which are documented in their own engine design document.
- **Language:** Rust. Engine and CLI share one Cargo workspace, so the wire
  types are a single source of truth.

---

## 1. Purpose, audience, and success criteria

`poker-cli` is a command-line interface for No-Limit Texas Hold'em (NLHE) that is
usable by **both a human player and an AI agent**, in the spirit of the
Playwright CLI: small, discrete, scriptable commands over a shared session.

**Success criteria**

- An agent can play or observe an entire hand using only CLI commands and their
  output.
- A human can read the same session comfortably, without requesting JSON.
- The CLI never decides poker legality; the engine is the single authority.
- No player's hole cards are ever disclosed to anyone but that player (the sole,
  explicit, admin-gated exception is `--reveal`, see §6.4).

**Audiences**

| Audience | Needs |
|---|---|
| Human player | Readable table view; short commands; discoverable via `help`. |
| AI agent | Stable versioned JSON; engine-provided `legal_actions`; blocking `wait`; structured errors with stable exit codes. |
| Spectator / evaluator agent | Read-only, never sees hole cards; can `feed`/`wait`/`status`. |

**Non-goals for v1:** web UI, remote/multi-host play, poker variants other than
NLHE, situational bot AI beyond seat count, rebuys, crash recovery of in-flight
tables, out-of-turn pre-committed actions.

---

## 2. Architecture and process model

### 2.1 Processes

- **Server** (`poker-cli serve`) is a long-lived process that owns all tables and
  the engine. It is the single writer per table.
- **Client** is any other `poker-cli` invocation: it opens the Unix socket, sends
  a `hello` handshake, sends one request, reads one response (or a stream for
  `feed`/`wait`), and exits.
- `start` requires a reachable server. **`--autostart`** (or `POKER_AUTOSTART=1`)
  lazily spawns a server if none is reachable; without it, `start` fails with
  `unreachable` (exit 4). There is no implicit daemon.

### 2.2 Concurrency model

Poker turn order is strictly sequential — exactly one seat acts at a time, and
each action changes the state the next actor sees. There is therefore no
parallelism to exploit. The server serializes all seat actions in
game-defined order (preflop: UTG → … → SB → BB; postflop: SB → BB → … → button).
No client-side locking is ever required. Out-of-turn pre-commit actions are out
of scope for v1.

### 2.3 Workspace layout

```
poker-core    # engine: rules, betting legality, hand evaluation, equity, RNG/dealing
poker-cli     # binary: arg parsing, rendering, Unix-socket client
poker-server  # server: sessions, tables, tokens, clock, event log, persistence
```

`poker-server` may be a module of `poker-cli`; either is acceptable. The public
wire types live in a shared module so the protocol, the human rendering, and the
on-disk history share one definition.

### 2.5 Engine boundary

This spec covers the CLI and server; the engine is specified separately. The
split is:

- **Engine (`poker-core`)** owns poker truth: dealing/RNG (seeded), betting
  legality and `legal_actions`, min-raise bounds, hand evaluation, showdown,
  equity, and the rules of a hand's state machine.
- **Server** owns sessions and process concerns: tables and seats, tokens and
  roles, the turn clock, the canonical event log and `seq`, persistence,
  redaction, and serializing actions in game order.
- **CLI** owns argument parsing (shape only) and rendering (text/JSON) — never
  poker judgement.

The server calls the engine for every rule decision; the wire contract in §2.4
is the only coupling the engine spec needs to agree with.

> **Not poker judgement.** "Never makes poker judgement" means *rule
> adjudication* — legality, turn, and winners. A read-only statistic computed
> over data the caller already legitimately holds (such as `equity`) is not
> adjudication.

### 2.4 Wire protocol

- Transport: **Unix domain socket**, path recorded in `.poker` (default
  `$XDG_RUNTIME_DIR/poker/<name>.sock`, overridable with `serve --socket PATH`).
- Framing: **newline-delimited JSON** (JSONL), UTF-8, one JSON object per line.
- Handshake: the client's first line is `hello`; the server replies with a
  `hello` ack carrying the negotiated protocol version. A version mismatch is
  rejected with `protocol_version` (exit 5), never misparsed.
- Correlation: every request carries a `req` id; every response echoes it.

```
→ {"v":1,"hello":{"client":"poker-cli/0.1","protocol":1,"token":"<token?>"}}
← {"v":1,"hello":{"server":"poker/0.1","protocol":1,"socket":"/run/user/1000/poker/main.sock"}}

→ {"v":1,"req":"r-42","cmd":"choose","table":"hero",
   "args":{"action":"raise","to":60},"if_seq":118}
← {"v":1,"req":"r-42","ok":true,"data":{...}}
```

Failure response:

```
← {"v":1,"req":"r-42","ok":false,
   "error":{"code":"not_your_turn","message":"seat 2 is not on turn",
            "turn":{"seat":1,"deadline_ms":11840},"legal_actions":{...}}}
```

Streaming commands (`feed`, `wait`) send the normal response line and then emit
**event lines** until the client disconnects or a terminal condition is reached.
The server renders each stream **per subscriber**, redacting hole cards the
subscriber may not see (§6.4).

---

## 3. Identity, roles, and addressing

### 3.1 Roles

| Role | Capability |
|---|---|
| **player** | Read/write: `choose`, receive own hole cards, `sit-out`/`sit-in`, `leave`. |
| **spectator** | Read-only: `snapshot` (no hole cards), `status`, `wait`, `feed`. |
| **admin / host** | Everything a player can do plus `step`, `replay`, `end`, `--reveal`, and table control. |

Tokens are opaque strings issued by the server at `start` (admin token) and
`join` (player/spectator token). A token's role is fixed at issue time.

### 3.2 The `.poker` file

Written by `start` (admin token) or `join` (player/spectator token), with
permissions `0600` because it holds a token. `--no-write` suppresses it.

```json
{
  "v": 1,
  "role": "player",
  "server": "unix:///run/user/1000/poker/main.sock",
  "table": "hero",
  "seat": 2,
  "token": "…",
  "format": "json"
}
```

### 3.3 Resolution ladder

Every client resolves its endpoint and identity from, in order:

1. **Explicit flags:** `--server`, `--table`, `--seat`, `--token`, `--format`.
2. **Environment:** `POKER_SERVER`, `POKER_TABLE`, `POKER_SEAT`, `POKER_TOKEN`,
   `POKER_FORMAT`.
3. **Nearest `.poker`**, found by walking up from the current directory.

Because a server hosts many tables, `--table` (or `POKER_TABLE`) is the
disambiguator. `cd` into a directory with a `.poker` file is a first-class way
to select a table.

### 3.4 Seats and joining

- `join --seat N` claims seat `N` if free; otherwise the server assigns the
  **lowest free seat**.
- `join --watch` yields a **read-only spectator** token and no seat.
- Re-`join` with an existing token **reattaches idempotently** to the same seat;
  it never consumes a new one.
- If the table no longer exists (in-memory tables do not survive a server
  restart), the client receives `not_found` (exit 7).

---

## 4. Command surface

### 4.1 Server and table

| Command | Semantics |
|---|---|
| `serve [--socket PATH] [--daemon]` | Run the server. Foreground in v1; `--daemon` reserved. |
| `start --table ID [options]` | Create a table; caller becomes host. See §10 for the full option list. |
| `tables` | List tables on the server: id, seats, state, whose turn. Discovery primitive. |
| `begin [--hands N \| --continuous]` | Host deals the next hand (`–hands`/`--continuous` per §5.2). |
| `end` | Host closes the table. Stops play; results final; seats release. The server keeps running. |

### 4.2 Seating

| Command | Semantics |
|---|---|
| `join [--table ID] [--seat N] [--as NAME] [--watch] [--no-write]` | Take a seat, or join as spectator with `--watch`. |
| `leave` | Vacate your seat. If a hand is in progress, your hand folds immediately and you sit out thereafter. |
| `sit-out` | Mark your seat present but **not dealt in** (chips kept, blinds skipped) until `sit-in`. |
| `sit-in` | Undo `sit-out`; you are dealt into the next hand. |

### 4.3 Playing and observing

| Command | Semantics |
|---|---|
| `snapshot [--reveal]` | Current, perspective-scoped truth of the table (§6.1). |
| `status` | Public seats digest (§6.2). |
| `choose <action> [--if-seq N]` | Act. Actions: `fold`, `check`, `call`, `raise <to> [--by N]`, `all-in`. |
| `equity [--iterations N] [--opponents K] [--seed HEX] [--exact]` | Fair Monte-Carlo equity for your hand (§6.3). |
| `wait [--turn] [--change] [--until EXPR] [--timeout D] [--since ID]` | Block until a condition holds (§7.3). |
| `feed [--since ID] [--types LIST] [--timeout D]` | Stream the table's event log (§7.2). |

**`choose` grammar and amount semantics.** `raise` takes a positional amount
meaning **raise *to* that total**; `--by N` means raise *by* `N`. The CLI
performs shape parsing only; the engine decides legality. Examples:

```
poker-cli choose fold
poker-cli choose check
poker-cli choose call
poker-cli choose raise 60          # raise to 60 total
poker-cli choose raise --by 60     # raise by 60
poker-cli choose all-in
```

**Validation is three-layered.** CLI = shape only. Engine = authoritative
legality. Server = return a structured error plus fresh `legal_actions`/`turn`
so a correctable failure can be fixed in one round trip.

### 4.4 Admin and step mode

| Command | Semantics |
|---|---|
| `flow <auto\|manual>` | Admin: set the table's flow. `auto` advances streets automatically; `manual` enables `step` (§5.4). |
| `step` | Admin: advance the engine exactly one transition (e.g. flop → turn). Only meaningful in manual flow (§5.4). |
| `replay --hand N [--table ID] [--step \| --auto]` | Admin: re-drive a stored hand, optionally transition by transition. |

### 4.5 History and meta

| Command | Semantics |
|---|---|
| `history [--table ID] [--hand N]` | List or fetch persisted hands (§9). |
| `help [COMMAND]` | Usage. |
| `version` | Client and (if reachable) server/protocol version. |

---

## 5. Table and hand lifecycle

### 5.1 States

```
waiting → ready → in-hand → between-hands → (in-hand …) → finished
```

- `waiting`: created; seats not yet filled (or `begin` not yet called).
- `ready`: all non-sitting-out seats are occupied; awaiting the host's `begin`.
- `in-hand`: a hand is being played.
- `between-hands`: hand complete; awaiting the next `begin`.
- `finished`: `end` was called, or only one seat holds chips.

### 5.2 `begin` is always explicit

Nothing deals itself. `begin` deals **exactly one hand**. `begin --hands N`
deals up to `N` hands back-to-back, and `begin --continuous` deals until `end`.
After each showdown, without `--hands`/`--continuous`, the table returns to
`between-hands`.

### 5.3 Streets advance automatically (auto flow)

In **auto** flow (default), the server advances streets as soon as the betting
round closes. Players only ever `choose` and observe; there is no player-facing
"advance" command.

### 5.4 Admin step mode (manual flow)

Realizing the approved "hybrid": the table has a **flow** setting.

- Set at creation with `start --flow auto|manual` (default `auto`), or toggled at
  runtime by the admin with `flow <auto|manual>`.
- In `manual` flow, the server does not auto-advance; the admin drives
  transitions with `step`. This is for debugging, deterministic testing, and
  `replay`.
- Step mode bypasses the turn clock, so debugging is never raced by a timer.

### 5.5 Turn clock

- Configured with `--turn-timeout D` (default **60s**; `0` disables) and an
  optional `--timebank T` extra pool per player.
- On expiry the server applies the standard fallback: **check if it is free,
  otherwise fold**.
- A timeout is emitted as an **event** (`timeout`), visible in `feed`, `wait`,
  and reflected in the next `snapshot`.
- Step mode ignores the clock.

### 5.6 Leaving, sitting out, and busting

- `leave` mid-hand folds that seat immediately and removes it thereafter.
- `sit-out` keeps the seat and chips but skips blinds and hands until `sit-in`.
- A seat whose stack reaches 0 is **sitting out**.
- When only **one** seat still holds chips, the table **automatically ends**. No
  rebuys in v1.
- Auto-`sit-out` after missed turns is **not** in v1 (candidate future flag
  `start --auto-sit-out K`).

---

## 6. View model and information access

### 6.1 `snapshot` (perspective-scoped)

For a player, `you` carries private data; `seats` carries public data only. A
spectator receives no `hole` field anywhere.

```json
{
  "schema": "poker.snapshot/1",
  "seq": 118,
  "revealed": false,
  "table": "hero",
  "hand": 7,
  "street": "flop",
  "board": ["Ah", "Kd", "7c"],
  "pots": {"main": 240, "side": []},
  "button": 3,
  "blinds": {"sb": 5, "bb": 10, "ante": 0},
  "current_bet": 20,
  "min_raise_to": 40,
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
     "status": "active", "last_action": "raise to 20"},
    {"seat": 3, "name": "bot-1", "stack": 1000, "bet": 0,
     "status": "active", "last_action": null}
  ],
  "history": [ {"street": "preflop", "seat": 1, "action": "raise to 20"} ],
  "events": [ {"seq": 117, "type": "action", "seat": 1, "action": "raise to 20"} ]
}
```

`legal_actions` is **engine-computed** and always describes the calling seat's
current menu, so an agent selects rather than guesses.

### 6.2 `status` (public digest)

A thin, always-safe view of public seat state — never leaks what `snapshot`
hides, and never contains hole cards.

```json
{
  "schema": "poker.status/1",
  "seq": 118,
  "table": "hero", "hand": 7, "street": "flop",
  "turn": {"seat": 2},
  "seats": [
    {"seat": 1, "name": "bob", "stack": 700, "bet": 20,
     "position": "BB", "status": "active", "last_action": "raise to 20"},
    {"seat": 2, "name": "alice", "stack": 940, "bet": 0,
     "position": "BTN", "status": "active", "last_action": null}
  ]
}
```

### 6.3 `equity` (fair equity)

Default mode is **fair Monte-Carlo**: your equity is simulated against
**unknown/random** opponent hands — exactly what a real player could compute.
The default run is `--iterations 40000`, which targets a half-width of about
0.005; the standard error is always reported so precision is honest.
Parameters: `--iterations N`, `--opponents K` (default = live opponents),
`--seed HEX` (deterministic, reproducible), `--exact` (enumerate when cheap).

```json
{
  "schema": "poker.equity/1",
  "mode": "monte-carlo",
  "iterations": 40000,
  "opponents": 2,
  "seed": "0123456789abcdef",
  "equity": 0.4132,
  "se": 0.0019,
  "win": 0.4110,
  "tie": 0.0044,
  "lose": 0.5846
}
```

`equity`, `win`, `tie`, and `lose` are fractions in `[0,1]` with
`win + tie + lose ≈ 1`. `seed` is always present: the effective seed, whether
supplied or derived locally when `--seed` was omitted.

With `--exact`, the engine enumerates every scenario with zero simulation
error, so the result instead carries `"mode": "exact"`, `"scenarios"` in place
of `"iterations"`, and `"se": 0`:

```json
{
  "schema": "poker.equity/1",
  "mode": "exact",
  "scenarios": 990,
  "opponents": 1,
  "seed": "0123456789abcdef",
  "equity": 1.0,
  "se": 0.0,
  "win": 1.0,
  "tie": 0.0,
  "lose": 0.0
}
```

If `--exact` would need more than **1,100,000** scenarios (multiway flop and
above), the command fails with `too_many_scenarios` (exit 15) rather than
hanging; use Monte-Carlo for those spots.

Human rendering prints percentages (`equity 41.32%`,
`win 41.10% / tie 0.44% / lose 58.46%`) and shows `seed` only when `--seed`
was explicitly passed.

An admin-only **oracle** mode (equity against the actual known cards) is
documented as an off-by-default extension, reachable only with an admin token
and tagged in output. It is not required for v1.

### 6.4 Privacy invariant

**No hole card ever leaves the server except to the seat that owns it.** This
governs `snapshot`, `feed`, `wait`, and persisted history alike.

- `snapshot` is perspective-scoped; a spectator's snapshot has no `hole` field.
- `feed`/`wait` streams are redacted per subscriber.
- The **only** exception is admin `snapshot --reveal`, which requires a valid
  admin token, shows all hole cards, and sets `"revealed": true` so the output
  can never be mistaken for a normal view. `--reveal` is never on by default and
  is rejected for players and spectators.

---

## 7. Events and streaming

### 7.1 The event stream

Each table has one canonical, append-only event log. Every state change emits an
event and bumps the table's monotonic **`seq`**. The snapshot's `seq`, the
`feed --since` cursor, the `wait` cursor, and `choose --if-seq` all use this one
counter.

Event types (v1): `table.created`, `seat.joined`, `seat.left`, `seat.sat_out`,
`seat.sat_in`, `hand.begin`, `blinds.posted`, `cards.dealt`, `action`,
`street.advanced`, `timeout`, `showdown`, `hand.complete`, `table.finished`.

```json
{"schema":"poker.event/1","seq":118,"type":"action","seat":1,
 "street":"preflop","action":"raise to 20"}
```

### 7.2 `feed`

Streams events as JSONL (or human lines) from `--since ID` (default: from the
current state) until the client disconnects, the table finishes, or `--timeout`
elapses. `--types` filters by event type. `feed` is the human/observer live view
and the substrate for logging.

### 7.3 `wait`

Blocks server-side until a condition holds, then prints the matching state and
exits. It is the agent's turn-taking primitive, avoiding polling.

- `--turn`: return when the calling seat is on turn.
- `--change`: return on any state change affecting the caller.
- `--until EXPR`: return when an expression over the state holds (e.g.
  `street=river`, `hand=8`).
- `--timeout D`: bound the wait; on expiry exit **14** (`wait_timeout`).
- `--since ID`: resume from a cursor.

A spectator may `wait` on public conditions only.

---

## 8. Output, errors, and schema

### 8.1 Format selection

```
default:   stdout is a TTY  → text (pretty tables)
           stdout is not a TTY → json
override:  --format json | jsonl | text      (flag beats env)
           POKER_FORMAT=json                  (env pins it, immune to TTY)
```

Pinning via `--format`/`POKER_FORMAT` neutralizes the one hazard of TTY
detection: an agent that allocates a pseudo-TTY still receives JSON.

**One schema, two renderings.** Human output is a rendering of the same
versioned JSON model, not a separately hand-built string. In Rust, both derive
from one shared type, so the two audiences cannot drift.

### 8.2 Streams

- **Data → stdout.** Diagnostics and errors → **stderr.** `poker-cli snapshot |
  jq` therefore never breaks on an error.
- Human mode errors print a readable message; JSON/JSONL mode errors print a
  JSON object with the same `code`.

### 8.3 Exit codes and error classes

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
| 15 | `too_many_scenarios` | `--exact` needs more than 1,100,000 scenarios; use Monte-Carlo |

Any correctable failure carries the context to correct it: `illegal_action`,
`not_your_turn`, and `stale` include fresh `legal_actions`/`turn`.

### 8.4 Schema and protocol versioning

- Every payload names its version: `poker.snapshot/1`, `poker.status/1`,
  `poker.equity/1`, `poker.event/1`, and a protocol `v` on the envelope.
- Additive fields are backward compatible within a major version; breaking
  changes bump the version. The `hello` handshake negotiates the protocol and
  rejects mismatches before any command is interpreted.

---

## 9. Persistence

- Completed hands are written as one JSON document each to
  `.poker/history/<table>/hand-<n>.json`, using the **same event schema** `feed`
  emits.
- `history` lists or fetches stored hands; `replay` (admin) re-drives a stored
  hand, pairing with `step` for transition-by-transition walkthroughs.
- `--seed` makes a session's deals reproducible, so a stored hand can be
  re-driven deterministically.
- **In-flight tables are not crash-recovered in v1.** The canonical event stream
  is designed as a single sink so that full event-sourced recovery is a later
  upgrade (turn the sink into a source) without changing the wire contract.
- Server diagnostic logs go to stderr and, optionally, a file
  (`serve --log PATH`).
- The history directory is resolved against the **server's** working directory,
  since the server (not the client) writes hands.

---

## 10. Configuration reference

### 10.1 `start` options

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
| `--variant` | poker variant | `nlhe` (only value in v1) |
| `--flow auto\|manual` | auto-advance or admin-stepped | `auto` |
| `--autostart` | lazily spawn a server if none reachable | off |
| `--no-write` | do not write a `.poker` file | off |

### 10.2 `serve` options

`--socket PATH` (default `$XDG_RUNTIME_DIR/poker/<name>.sock`), `--daemon`
(reserved), `--log PATH`.

### 10.3 Environment

`POKER_SERVER`, `POKER_TABLE`, `POKER_SEAT`, `POKER_TOKEN`, `POKER_FORMAT`,
`POKER_AUTOSTART`.

---

## 11. Explicitly out of scope for v1

Bots beyond a seat count; admin oracle equity (documented extension);
out-of-turn pre-commit (`choose --when-turn`); auto-`sit-out`; HTTP facade;
crash recovery of in-flight tables; poker variants beyond NLHE; rebuys;
multi-host play.

---

## 12. Testing strategy

- **End-to-end:** spawn the real `poker-cli` binary against an in-process server
  over a temporary socket; drive whole hands (`wait --turn` → `choose` → …).
- **Determinism:** seeded sessions produce identical deals, events, and history
  files; `replay` reproduces a stored hand exactly.
- **Golden files:** the JSON schema of `snapshot`, `status`, `equity`, `event`,
  and `history` is pinned.
- **Contract:** every error class yields its specified exit code and `error.code`
  in both text and JSON modes; stdout stays clean on failure.
- **Privacy:** a spectator, a non-owning player, `feed`, and `wait` never receive
  another seat's hole cards; `--reveal` works only with an admin token and is
  tagged.
- **Protocol:** version mismatch is rejected before command interpretation;
  `stale`/`if_seq` guards behave as specified; concurrent same-seat actions
  produce exactly one success.

---

## 13. Deliverables

1. This design spec.
2. An implementation plan (via the writing-plans skill).
3. An **agent skill document** at `skills/poker-cli/` (`SKILL.md` plus a
   `references/` file), shipped as a Pi package via the repo-root
   `package.json` (`pi.skills` + the `pi-package` keyword), so users can install
   it with `pi install git:github.com/<owner>/poker-cli`. It describes the
   canonical agent loop (`wait --turn` → `snapshot` → `choose`), the exit-code
   contract, and the `POKER_FORMAT=json` bootstrap. Status: **draft**, pending
   validation against the implemented binary.
