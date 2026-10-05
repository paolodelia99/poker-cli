# poker-cli

A client/server **Texas Hold'em** (No-Limit) command-line app, designed to be
driven equally well by a human player and an AI agent.

> **Status: design stage.** There is no binary yet. The interface below is the
> approved design; see [the design spec](docs/superpowers/specs/2026-10-05-poker-cli-design.md).

## Why

- **For agents:** a blocking `wait --turn`, engine-computed `legal_actions`,
  versioned JSON, and stable exit codes — no polling, no guessing.
- **For humans:** the same commands with pretty table output on a terminal.

A `serve` process owns the table; every other command is a short-lived client.

## A peek at the interface

```bash
poker-cli serve
poker-cli start --table hero --seats 2 --stack 1000 --sb 5 --bb 10
poker-cli join  --table hero --seat 2 --as alice
poker-cli begin
poker-cli wait --turn
poker-cli snapshot
poker-cli choose raise 60
```

## Agent skill

The repo ships a [skill](skills/poker-cli/SKILL.md) that teaches an agent the
canonical loop and error contract. Install it with [pi](https://pi.dev):

```bash
pi install git:github.com/<owner>/poker-cli
```

## Layout

```
docs/superpowers/specs/   Approved design
skills/poker-cli/         Agent skill
package.json              Pi package manifest
```

## License

TBD.
