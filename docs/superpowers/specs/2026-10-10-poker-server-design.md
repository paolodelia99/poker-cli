# Poker Server — Design Specification

- **Status:** Approved (brainstorming complete, pending final user review)
- **Date:** 2026-10-10
- **Scope:** The `poker-server` crate: sessions, tables/seats, token/role
  issuance, the turn clock, the canonical event log and `seq`, per-subscriber
  redaction, action serialization, the Unix-socket JSONL protocol, and hand
  persistence.
- **Contract source:** `docs/superpowers/specs/2026-10-05-poker-cli-design.md`
  (the CLI design). Where this document and the CLI spec disagree, the CLI spec
  wins until it is deliberately revised.
- **Dependencies (separate specs):** `poker-core` (the rule oracle) and
  `poker-bots` (bot strategies). Both are treatable as black boxes; their call
  seams are pinned in §3, and each gets its own spec and implementation plan.
- **Language:** Rust. One Cargo workspace shared with the CLI and engine.

---

## 1. Purpose, audience, and success criteria

`poker-server` is the long-lived process that owns poker tables and serializes
everything that happens at them. It is the only writer of a table's state and
the only authority on process concerns: identity, time, the event log, and
persistence. It never decides poker truth — that is `poker-core`'s job.

**Success criteria**

- A client can create a table, seat players/spectators, play whole hands, and
  observe them live using only the wire protocol, with the CLI spec's command
  surface served faithfully.
- **No hole card ever leaves the server except to the seat that owns it** —
  across `snapshot`, `feed`, `wait`, and persisted history — with the sole
  exception of admin `snapshot --reveal`, tagged `"revealed": true`.
- A table's state is mutated in exactly one place, in strict game order, with a
  monotonic `seq` that `feed`/`wait`/`choose --if-seq` can rely on.
- Given a seed, a run's event log and persisted hand documents are
  byte-identical, so hands are reproducible and testable.
- The server survives slow or malicious subscribers without unbounded memory
  growth or silent event loss.

**Audiences**

| Audience | Interaction |
|---|---|
| Human player | via the CLI, which renders the server's JSON |
| AI agent | stable versioned JSON; engine-computed `legal_actions`; blocking `wait` |
| Spectator / evaluator | read-only views and streams, never hole cards |
| Admin / host | table control, `step`, `replay`, `--reveal` |

---

## 2. Architecture and process model

### 2.1 Relationship to the CLI

`poker-server` is a library crate. The `poker-cli` binary's `serve` subcommand
is a thin entry point that calls `poker_server::run(config)`. The server never
parses human output and never renders text; it speaks only the JSONL protocol.
The CLI performs argument-shape parsing and rendering, per the CLI spec §2.5.

### 2.2 Runtime and concurrency

The server runs on a Tokio runtime and observes the CLI spec's serialization
mandate (one writer per table, strict game order) structurally:

- **One task per client connection**, owning that connection's socket, handshake
  state, and at most one in-flight request (except while streaming — §5.5).
- **One actor task per table**, owning `TableState`. The actor is the *only*
  writer; every mutation arrives as an intent on the table's `mpsc` mailbox and
  is processed to completion before the next. There is no shared mutable table
  state and no per-table lock.
- **One `broadcast` channel per table** carrying canonical events to
  subscribers (`feed`/`wait`). Subscribers redact per observer (§7.3).
- **A registry** mapping `TableId -> TableHandle` (mailbox + broadcast +
  metadata), guarded for concurrent lookup, consulted by the dispatcher to route
  requests. It holds only handles, never table state.
- **A single persistence writer task** consuming finished-hand documents
  (§10.3), so no table actor ever blocks on disk I/O.
- **Timers via `tokio::time`**, one active turn timer per table (§8).

The design has no parallel game logic: turn order is sequential, so the actor
mailbox *is* the serialization guarantee.

### 2.3 Components (module seams)

| Module | Responsibility |
|---|---|
| `config` | `serve` options, socket path, log path, history anchor |
| `listener` | bind the Unix socket, accept loop, spawn a connection task |
| `session` | `hello` handshake, protocol negotiation, request/response I/O, streaming |
| `protocol` | wire envelope, request/response/error types, version constant |
| `registry` | table creation, lookup, and removal; `tables` listing |
| `table` | `TableState`, intent handling, engine transitions, lifecycle |
| `events` | `Event` type, `seq` assignment, append, broadcast |
| `views` | the redaction choke point: `view_for`, `event_for` |
| `clock` | turn deadline, timebank, timeout fallback |
| `tokens` | CSPRNG issuance, `(table, seat, role)` lookup, role checks |
| `conditions` | `wait --until` predicate registry |
| `persistence` | hand-document builder, writer task, atomic file writes |
| `errors` | `ErrorCode`, server-side error construction, CLI code mapping |

Each module's public surface is small and testable in isolation: the `table`
actor can be driven by sending intents to its mailbox with no socket involved.

---

## 3. Pinned dependency seams

These are the interfaces the server is allowed to call. Each dependency owns its
own spec; this section is the contract that spec must satisfy.

### 3.1 `poker-core` (engine)

`poker-core` is pure: **no clock, no sockets, no `seq`, no I/O**. The server
calls:

```
new_hand(params, seed)        -> Hand
legal_actions(&Hand, seat)    -> LegalActions
apply(&Hand, seat, action)    -> Result<Transition, EngineError>
auto_advance(&Hand)           -> Transition            // betting round closed
step(&Hand)                   -> Transition            // one manual transition
equity(input, iters, seed, exact) -> Equity
```

- `Transition = { hand: Hand, events: Vec<EngineEvent>, phase: Phase }`.
- `EngineEvent` is an event **without `seq`**. The server stamps `seq`.
- `LegalActions` is exactly the shape surfaced in `poker.snapshot/1`
  (`fold`/`check`/`call`/`raise`/`all_in`) — the server neither computes nor
  rewrites it.
- Engine events: `hand.begin`, `blinds.posted`, `cards.dealt`, `action`,
  `street.advanced`, `showdown`, `hand.complete`.
- `equity` lives in a `poker-core` module (the probability calculator). It is
  consumed **client-side** by `prob` (§5.7); the server does not expose it.
- A hand's `seed` is engine input only and **is never persisted** (§10.2).

### 3.2 `poker-bots` (strategies)

`poker-bots` depends only on `poker-core`. It exposes a `Strategy` trait:

```
trait Strategy {
    fn choose(&mut self, view: &BotView, rng: &mut Rng) -> Action;
}
```

- `BotView` is a perspective-scoped view of the bot's own seat and the public
  table; the menu is `legal_actions`.
- The RNG is derived from the hand seed, so bot play is deterministic under a
  seeded session.
- v1 ships one default strategy. `start --bots K` selects it; there is no CLI
  flag to choose a strategy yet (a future CLI-spec revision).
- The server owns *when* a bot acts (its turn, no clock); `poker-bots` owns
  *what* it plays.

---

## 4. Identity, roles, tokens, and discovery

### 4.1 Tokens

- Issued at `start` (admin/host token) and `join` (player or spectator token).
- A token is an opaque, cryptographically random string of at least 128 bits
  (URL-safe encoding).
- Tokens live **in memory only** and map to `(table, seat?, role)`. A token's
  role is fixed at issue; every request re-checks the role.
- Tokens are never written to the event log, history, or logs.
- Because tables are in-memory, a server restart invalidates every token and
  every table; clients receive `not_found` (exit 7).

### 4.2 Roles and authorization

| Request | Required authority |
|---|---|
| `hello`, `version` | none |
| `tables` | none (public summaries only) |
| `start` | none (the server issues the admin token) |
| `join` | none (the server issues a player/spectator token) |
| `status` | none (public digest only) |
| `history` | none (public hand documents only) |
| `snapshot` | token optional; without one, a spectator-scoped view |
| `wait`, `feed` | token optional; without one, public conditions/events only |
| `choose`, `leave`, `sit-out`, `sit-in` | player token (own seat) |
| `begin`, `end`, `flow`, `step`, `replay`, `snapshot --reveal` | admin token |

The Unix socket's filesystem permissions are the outer boundary; v1 adds no
further network authentication.

### 4.3 Addressing

Addressing (flags → env → nearest `.poker`) is resolved **client-side** per CLI
spec §3.3. The server sees only resolved requests. The `.poker` file is written
and read by the client; **the server never writes it**. The server's `hello`
ack reports its socket path so clients can record it.

### 4.4 Tables are ephemeral

A table exists only for the life of the server process. In-flight tables are not
crash-recovered in v1; the canonical event stream is designed as a single sink
so event-sourced recovery is a later upgrade that turns the sink into a source
without changing the wire contract.

---

## 5. Wire protocol

### 5.1 Transport and framing

- Unix domain socket, path from `serve --socket PATH` (default
  `$XDG_RUNTIME_DIR/poker/<name>.sock`).
- Newline-delimited JSON (JSONL), UTF-8, one object per line.
- Request errors never corrupt the stream: a malformed line yields a structured
  error and the connection continues.

### 5.2 Handshake and versioning

```
→ {"v":1,"hello":{"client":"poker-cli/0.1","protocol":1,"token":"<token?>"}}
← {"v":1,"hello":{"server":"poker/0.1","protocol":1,"socket":"/run/user/1000/poker/main.sock"}}
```

The first client line must be `hello`. A protocol mismatch is rejected with
`protocol_version` (exit 5) **before any command is interpreted**. The `hello`
may carry a token; if absent, the connection is anonymous/spectator.

### 5.3 Correlation

Every request carries a `req` id; every response echoes it. There is at most one
in-flight non-streaming request per connection; the server processes requests
for one connection in order.

```
→ {"v":1,"req":"r-42","cmd":"choose","table":"hero",
   "args":{"action":"raise","to":60},"if_seq":118}
← {"v":1,"req":"r-42","ok":true,"data":{"schema":"poker.ack/1","seq":121}}
```

### 5.4 Command dispatch

`cmd` selects a handler; `table` (or the token's table) selects the actor. The
dispatcher enforces the §4.2 authority matrix before touching any table.

| `cmd` | Streaming | Notes |
|---|---|---|
| `start`, `tables` | no | registry-level |
| `begin`, `end`, `flow`, `step` | no | admin; actor intent |
| `join`, `leave`, `sit-out`, `sit-in` | no | seating; actor intent |
| `snapshot`, `status` | no | read; redacted view |
| `choose` | no | actor intent; ack |
| `wait`, `feed` | **yes** | subscribe, then stream |
| `replay` | no | isolated shadow hand (§9.4) |
| `history` | no | reads persisted documents |
| `version` | no | server/protocol version |

`prob`, `help`, and `version`'s client half are handled by the CLI, not the
server. `prob` issues only a normal state read and computes locally.

### 5.5 Streaming contract (`feed`, `wait`)

`feed`/`wait` send the normal response line, then event (or snapshot) lines
until a terminal condition or disconnect.

- **`feed`** streams `poker.event/1` lines. With no `--since`, it starts *after
  the current `seq`* (future events only); `--since ID` replays from the stored
  cursor; `--types LIST` filters; `--timeout D` bounds it.
- **`wait`** blocks until a condition holds, then responds with the
  perspective-scoped `poker.snapshot/1` at the matching `seq`:
  - `--turn` — the caller's seat is on turn.
  - `--change` — any event since the cursor affects the caller's view.
  - `--until key=value` — the bounded predicate registry (§5.6).
  - `--since ID` — starting cursor; `--timeout D` — bound.
- **Backpressure:** each subscriber has a **bounded** queue. A subscriber that
  falls behind is not silently truncated: the server ends the stream with a
  `stale` error carrying the **last successfully delivered `seq`**, so the
  client reconnects with `feed --since <that seq>` and loses nothing. Unbounded
  buffering is forbidden.
- A spectator or anonymous client may `wait` on public conditions and receives
  redacted events only.

### 5.6 `wait --until` predicate registry

`--until` accepts a single `key=value` comparison over a fixed key set — no
boolean operators in v1:

| Key | Values |
|---|---|
| `street` | `preflop`/`flop`/`turn`/`river` |
| `hand` | hand number |
| `state` | `waiting`/`ready`/`in-hand`/`between-hands`/`finished` |
| `seq` | exact event sequence |
| `turn.seat` | seat number |
| `seat.<n>.status` | `active`/`folded`/`all-in`/`sitting-out` |

An unknown key or malformed expression is `usage` (exit 2). A predicate that
never holds is bounded by `--timeout` → exit 14.

### 5.7 `prob` is client-local

`prob` requires only the caller's hole cards, the board, and the opponent count
— all present in the caller's own snapshot. The client process links
`poker-core` and runs `equity` locally after a normal perspective-scoped state
read; the server never receives a `prob` request, never computes equity, and
stores nothing about it.

> **CLI-spec clarification:** §2.5's "the CLI never makes poker judgement" means
> *rule adjudication* — legality, turn, winners. A read-only statistic computed
> over data the caller already legitimately holds is not adjudication. This
> clarification should be mirrored in the CLI spec when it is next revised.

---

## 6. Events, `seq`, and redaction

### 6.1 The canonical log

Each table has one append-only event log. Every state change emits an event and
increments the table's monotonic `seq`, which is assigned **only by the table
actor**. The snapshot's `seq`, `feed --since`, the `wait` cursor, and
`choose --if-seq` all share this counter.

Event types and their producers:

| Producer | Events |
|---|---|
| server | `table.created`, `seat.joined`, `seat.left`, `seat.sat_out`, `seat.sat_in`, `timeout`, `table.finished` |
| engine (relayed) | `hand.begin`, `blinds.posted`, `cards.dealt`, `action`, `street.advanced`, `showdown`, `hand.complete` |

Every event is `poker.event/1`; the payload extends the CLI spec's shape:

```json
{"schema":"poker.event/1","seq":118,"type":"action","seat":1,
 "street":"preflop","action":"raise to 20"}
```

Representative payloads (illustrative, not exhaustive):

- `table.created`: `{seats, sb, bb, ante, button, stacks, flow}`
- `seat.joined`: `{seat, name, stack}` (spectators are not seated)
- `hand.begin`: `{hand, button, seats, stacks}`
- `blinds.posted`: `{sb:{seat,amount}, bb:{seat,amount}, antes:[...]}`
- `cards.dealt`: `{seat, hole:[...]}` (private) or `{street, board:[...]}`
- `action`: `{seat, street, action, amount?}`
- `street.advanced`: `{from, to, board}`
- `timeout`: `{seat, reason:"check"|"fold"|"timebank", ...}`
- `showdown`: `{reveals:[{seat,hole}], pots, winners}`
- `hand.complete`: `{hand, winners, stacks}`
- `table.finished`: `{reason}`

### 6.2 Transition discipline

Every accepted intent calls **one** engine transition that returns
`(new_state, events)`. The actor replaces its state, stamps each returned event
with the next `seq`, appends it to the log, and broadcasts it. No code path
mutates table state or emits an event outside this transition. This is what
makes the later event-sourced upgrade mechanical: replay is applying the stored
events through the projection.

### 6.3 Redaction — the single choke point

In-memory `TableState` and the log hold the **full truth**, so admin features
(`--reveal`, `step`, showdown evaluation) work. Exactly two functions convert
server state into bytes for a connection:

- `view_for(observer, &TableState) -> Snapshot` — perspective-scoped snapshot.
- `event_for(observer, &Event) -> Event` — perspective-scoped event.

Properties:

- A spectator's or non-owner's view has no `hole` field anywhere.
- `"revealed": true` is set only for admin `snapshot --reveal` (and rejected for
  players/spectators).
- `showdown` reveals only the cards the rules actually expose.
- **No other module may serialize a hole card.** The persistence projection
  (§10.2) uses the same public projection.

---

## 7. Table and hand lifecycle

### 7.1 States

```
waiting → ready → in-hand → between-hands → (in-hand …) → finished
```

- `waiting`: created; not all non-sitting-out seats are filled.
- `ready`: all active seats occupied; awaiting the host's `begin`.
- `in-hand`: a hand is in progress.
- `between-hands`: hand complete; awaiting the next `begin`.
- `finished`: `end` was called, or only one seat holds chips.

### 7.2 `begin` and flow

- `begin` is always explicit. `begin` deals exactly one hand;
  `begin --hands N` deals up to `N` back-to-back; `begin --continuous` deals
  until `end` or auto-end. Absent those, the table returns to `between-hands`
  after each showdown.
- `begin` with fewer than **two** active seats is rejected as
  `illegal_action` (no free exit code exists in CLI spec §8.3) with a message
  and fresh state.
- **Auto flow** (default): the actor calls `auto_advance` whenever a betting
  round closes; streets and showdown proceed with no player-facing "advance".
- **Manual flow** (`start --flow manual` / `flow manual`): the actor does not
  auto-advance; the admin drives transitions with `step`. Manual flow disables
  the turn clock.
- `step` advances exactly one engine transition and is meaningful only in
  manual flow; in auto flow it is `illegal_action`.

### 7.3 Seating, sitting out, and busting

- `join --seat N` claims seat `N` if free, else the lowest free seat. A bot seat
  reads as taken. `join --watch` issues a spectator token and no seat.
- Re-`join` with an existing token reattaches idempotently; it never consumes a
  new seat.
- `join` during a hand takes the seat but deals the player in from the **next**
  hand. `sit-in`/`sit-out` likewise affect only the next deal.
- `leave` during a hand folds that seat immediately and removes it after the
  hand; otherwise it is immediate.
- `sit-out` keeps the seat and chips but skips blinds and hands until `sit-in`.
  A zero stack is sitting out.
- When only one seat holds chips, the table auto-ends (`table.finished`,
  reason `single_chip_leader`). No rebuys in v1.
- Auto-`sit-out` after missed turns is not in v1. A disconnected client is not
  auto-sat-out either: if it is that seat's turn, the turn clock resolves it
  (check-if-free-else-fold).

### 7.4 `end`

- With no hand in progress, `end` takes effect immediately.
- If `in-hand`, the current hand **plays out to completion first** (no abandoned
  pots, no partial hand persisted), then the table goes `finished` and emits
  `table.finished` (reason `end`).
- A `finished` table serves read-only `snapshot`/`status`/`history` with its
  final state. Mutating commands (`choose`, `begin`, `join`, `sit-in/out`,
  `leave`, `step`, `flow`) return `table_finished` (exit 13). `wait` returns
  immediately with the final snapshot rather than blocking.

### 7.5 Bots

- `start --bots K` requires `K < seats`. Bots occupy real seats, count toward
  `ready`, post blinds, and cannot be joined by clients.
- On a bot's turn the actor calls `poker-bots` (§3.2) and applies the returned
  action through the same transition path, emitting a normal `action` event.
- Bots have no clock and emit no `timeout` events. With a seeded session, bot
  choices are deterministic.

---

## 8. Turn clock

- Configured with `--turn-timeout D` (default 60 s; `0` disables) and
  `--timebank T` (default 0, off).
- `deadline_ms` in snapshots and errors is **remaining milliseconds**, computed
  fresh per response — never an absolute timestamp.
- On main-clock expiry, if the acting player's timebank pool is non-empty the
  server consumes from it and extends the same turn, emitting a `timeout` event
  with reason `timebank`. Only an empty pool triggers the standard fallback:
  **check if it is free, otherwise fold**, emitted as a `timeout` event.
- The timebank is a per-player pool that **refills at `hand.begin`** and is
  shared across that hand's turns.
- Manual flow / `step` disables the clock; bots are never clocked.

---

## 9. Protocol semantics per command

### 9.1 Reads

- `snapshot` returns `poker.snapshot/1` via `view_for`; `--reveal` (admin only)
  sets `"revealed": true`.
- `status` returns the public `poker.status/1` digest; never contains hole
  cards.
- `tables` returns `poker.tables/1`: `{id, seats, occupied, state, turn}` per
  table.

### 9.2 `choose`

- Grammar per CLI spec §4.3 (shape parsed client-side); the engine decides
  legality.
- Success returns the minimal ack `{"schema":"poker.ack/1","seq":N}` — not a
  snapshot.
- `if_seq N` is compare-and-swap on `seq`: rejected as `stale` (exit 12) unless
  the table's current `seq` equals `N` at the instant the actor dequeues it.
- `not_your_turn`, `illegal_action`, and `stale` responses carry fresh `turn`
  and `legal_actions` so a correctable failure is fixed in one round trip.

### 9.3 `history`

- `history` lists or fetches persisted hands (§10). Documents are public; no
  authority beyond reachability is required.

### 9.4 `replay`

- Admin-only. The server loads a stored hand document into an **isolated shadow
  hand** (never a live table), re-drives it through the same `poker-core`
  transitions, and emits events to the admin client. `--step` advances one
  transition per call; `--auto` runs the whole hand.
- Replay reconstructs public truth only: a mucked hand cannot be revealed
  because it was never persisted. Replay never mutates a live table and never
  writes history.

---

## 10. Persistence

### 10.1 Location

- Completed hands are written to
  `.poker/history/<table>/hand-<n>.json`, **anchored at the server's working
  directory** per CLI spec §9. (A future `serve --data-dir` would require a CLI
  spec revision; it is not in v1.)

### 10.2 Document shape and privacy

```json
{"schema":"poker.hand/1","table":"hero","hand":7,
 "params":{"seats":6,"sb":5,"bb":10,"ante":0,"button":3,"stacks":{...}},
 "events":[ {"schema":"poker.event/1","seq":97,"type":"hand.begin", ...}, ... ]}
```

- The document is a **public projection**: board, blinds, actions, stacks, and
  only the cards the rules exposed at showdown. Mucked/undisclosed hole cards
  are never written.
- `params` are embedded so the file is self-describing and replayable without
  earlier hands.
- **The hand seed is never persisted** (it would let a reader reconstruct every
  hole card). `replay` re-drives from the recorded public events, so it needs no
  RNG.

### 10.3 Write path

- On `hand.complete`, the actor builds the document and hands it to the
  **single writer task** over a channel; the actor never blocks on disk.
- The writer writes to a temporary file in the same directory and `rename`s it
  into place, so a reader never sees a partial document.
- Server diagnostics go to stderr and optionally `serve --log PATH`.

---

## 11. Errors

The server returns `{"ok":false,"error":{"code":..., "message":..., ...}}`. The
CLI maps `code` to a process exit code per CLI spec §8.3; the server does not set
exit codes.

| `code` | Exit | Server condition |
|---|---|---|
| `internal` | 1 | unexpected bug |
| `usage` | 2 | malformed request/args, unknown command or predicate |
| `protocol_version` | 5 | `hello` mismatch |
| `auth` | 6 | bad/missing token, insufficient role |
| `not_found` | 7 | no such table/seat/hand |
| `not_your_turn` | 10 | action out of turn (+ fresh `turn`/`legal_actions`) |
| `illegal_action` | 11 | action not in the menu; `<2` seats at `begin`; `step` in auto flow |
| `stale` | 12 | `if_seq` mismatch; stream overflow (+ last delivered `seq`) |
| `table_finished` | 13 | mutating a finished table |
| `wait_timeout` | 14 | bounded `wait` expired without matching |

`no_session` (3) and `unreachable` (4) are produced client-side and never
appear on the wire. `illegal_action`, `not_your_turn`, and `stale` always carry
the context needed to correct the call.

---

## 12. Testing strategy

- **Actor unit tests** drive the table mailbox directly and assert
  `(new_state, events, seq)` without sockets.
- **Clock tests** use `tokio::time::pause()`/`advance()` — never real waits.
- **Protocol E2E** spawns the real server on a temporary socket and drives whole
  hands across multiple connections.
- **Determinism:** a seeded run produces a byte-identical event log and hand
  document; `replay` reproduces it exactly.
- **Privacy:** assert no other seat's hole card appears in any subscriber's
  bytes — spectator, non-owning player, `feed`, `wait` — nor in the on-disk
  history file.
- **Backpressure:** force a lagging subscriber, assert `stale` with the last
  delivered `seq`, then assert a successful `feed --since` resync.
- **Golden JSON** fixtures pin `poker.snapshot/1`, `poker.status/1`,
  `poker.event/1`, and `poker.hand/1`.
- **Persistence:** atomic-rename test (no partial file after a killed write) and
  a `params` round-trip.
- **Contract:** every error class yields the specified `code`; stdout-style
  cleanliness is verified through the CLI's JSON mode.

---

## 13. Explicitly out of scope for v1

- Crash recovery / event-sourced recovery (designed-for, not built).
- Remote or multi-host play; HTTP facade.
- Poker variants beyond NLHE; rebuys.
- Admin equity oracle (a documented, off-by-default extension).
- Out-of-turn pre-commit actions; auto-`sit-out`; `serve --daemon`.
- Strategy selection flags for `poker-bots`.

---

## 14. Deliverables

1. This design spec.
2. An implementation plan (via the writing-plans skill) for `poker-server`.
3. Separate specs and plans for `poker-core` and `poker-bots`, each satisfying
   the seams pinned in §3.
