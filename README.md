# poker-cli

A client/server **Texas Hold'em** (No-Limit) command-line app, designed to be
driven equally well by a human player and an AI agent.

> **Status: design stage.** There is no binary yet. The commands below are the
> approved design; see
> [the CLI spec](docs/superpowers/specs/2026-10-05-poker-cli-design.md) and
> [the server spec](docs/superpowers/specs/2026-10-10-poker-server-design.md).

## Why

- **For agents:** a blocking `wait --turn`, engine-computed `legal_actions`,
  versioned JSON, and stable exit codes — no polling, no guessing.
- **For humans:** the same commands with pretty table output on a terminal.

## How it works

One long-lived **`serve` process** owns every table and the engine. Every other
command is a **short-lived client**: it opens the Unix socket, does one thing,
and exits. Because the transport is a Unix socket, everyone at a table is on the
**same machine** in v1 (remote play is a non-goal).

Roles are decided when you get a token:

| Role | Can do |
|---|---|
| **player** | `choose`, see your own hole cards, `sit-out`/`sit-in`, `leave` |
| **spectator** | read-only `snapshot`/`status`/`wait`/`feed`, never sees hole cards |
| **admin / host** | everything a player can, plus `begin`, `end`, `flow`, `step`, `replay`, `--reveal` |

## Start a table (host)

The server runs once; tables live inside it. You become the host (admin) of any
table you create.

```bash
# Terminal 1 — the server. Leave it running.
poker-cli serve --socket /run/user/1000/poker/main.sock

# Terminal 2 — host creates a table and writes a .poker here.
export POKER_SERVER=unix:///run/user/1000/poker/main.sock
poker-cli start --table hero --seats 6 --stack 1000 --sb 5 --bb 10
# → table "hero" created; you are host. Wrote ./.poker
```

Your `.poker` file holds your **admin token** and is written `0600`. **Do not
share it** — other players only need the socket path and the table name.

## Join a table

Anyone on the machine joins the same socket and table. `join` mints them their
own token; no one needs your admin token.

```bash
# A human player
export POKER_SERVER=unix:///run/user/1000/poker/main.sock
export POKER_TABLE=hero
poker-cli join --seat 2 --as bob

# An AI agent: pin JSON output first, then the same command
export POKER_FORMAT=json
poker-cli join --seat 3 --as agent-1

# A read-only spectator / evaluator (no seat, no hole cards)
poker-cli join --watch --as observer
```

- Omit `--seat` and the server assigns the lowest free seat.
- Re-running `join` with the same token reattaches to the same seat, idempotently.
- Discovery: `poker-cli tables` lists what is on the server, with seats, state,
  and whose turn it is.

## Tutorial: one hand, start to finish

Three players at a 3-seat table with blinds 5/10: **alice** (host, button),
**bob** (small blind), **agent-1** (big blind). All three have 1000 chips.

```bash
# alice (host)
poker-cli start --table hero --seats 3 --stack 1000 --sb 5 --bb 10

# bob
poker-cli join --seat 2 --as bob

# agent-1
poker-cli join --seat 3 --as agent-1

# alice deals one hand (nothing deals itself — begin is always explicit)
poker-cli begin
```

The server posts blinds (bob 5, agent-1 10) and deals hole cards privately.
Preflop action runs from the seat left of the big blind around to the big
blind; at a 3-seat table that means **alice (button) acts first**. Everyone
plays the same loop:

```bash
poker-cli wait --turn     # block until it is my turn (do not poll)
poker-cli snapshot        # read you.legal_actions, then pick exactly one
poker-cli choose <action>
```

**Preflop** — pot 30 after everyone calls/checks:

```bash
# alice (button)
poker-cli wait --turn
poker-cli choose call        # call 10

# bob (small blind) has 5 in, adds 5
poker-cli wait --turn
poker-cli choose call

# agent-1 (big blind) can check; check is free
poker-cli wait --turn
poker-cli choose check
```

Betting round closes → the **flop** is dealt automatically (auto flow).
Postflop order is small blind first, so **bob acts first**.

**Flop** (`Kh 7c 2d`):

```bash
poker-cli choose check       # bob
poker-cli choose check       # agent-1
poker-cli choose raise 20    # alice bets "to 20 total"
poker-cli choose call        # bob calls; pot 70
poker-cli choose fold        # agent-1 folds
```

**Turn** (`9s`) and **river** (`3h`) — both check around:

```bash
poker-cli choose check       # bob, turn
poker-cli choose check       # alice, turn
poker-cli choose check       # bob, river
poker-cli choose check       # alice, river
```

**Showdown:** alice's `Ks Qd` (pair of kings) beats bob's `8c 8d` (pair of
eights). Alice takes the 70-chip pot. The server writes the hand to
`.poker/history/hero/hand-1.json` (relative to the **server's** working
directory), and the table returns to `between-hands`.

Deal the next hand with another `poker-cli begin`, or run several:

```bash
poker-cli begin --hands 10    # ten hands back-to-back
poker-cli begin --continuous  # until `end` or one player holds all the chips
poker-cli end                 # host closes the table (server keeps running)
```

### What an agent sees

With `POKER_FORMAT=json`, `snapshot` is machine-readable, and the menu is
computed by the engine — the agent selects from it rather than guessing:

```json
{"schema":"poker.snapshot/1","seq":118,"street":"flop",
 "board":["Kh","7c","2d"],"current_bet":0,"turn":{"seat":2},
 "you":{"seat":2,"hole":["8c","8d"],"stack":990,
        "legal_actions":{"fold":true,"check":true,"call":0,
                         "raise":{"min":10,"max":990},"all_in":990}}}
```

A rejected action comes back with fresh context so the agent self-heals in one
step (data goes to stdout, the error to stderr):

```bash
poker-cli choose raise 1000
# exit 11, error.code "illegal_action", plus fresh legal_actions / turn
```

Agents can also estimate equity locally from their own private state:

```bash
poker-cli prob --iterations 50000    # fair Monte-Carlo vs. unknown hands
```

For the full command list, exit codes, and JSON shapes, see the
[agent skill reference](skills/poker-cli/references/poker-cli-reference.md).

## Agent skill

The repo ships a [skill](skills/poker-cli/SKILL.md) that teaches an agent the
canonical loop and error contract. Install it with [pi](https://pi.dev):

```bash
pi install git:github.com/<owner>/poker-cli
```

## Layout

```
docs/superpowers/specs/   Approved design specs
skills/poker-cli/         Agent skill
package.json              Pi package manifest
```

## License

TBD.
