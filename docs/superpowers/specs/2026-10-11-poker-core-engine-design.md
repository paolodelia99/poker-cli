# Poker Core Engine — Design Specification

- **Status:** Approved (brainstorming complete, pending final user review)
- **Date:** 2026-10-11
- **Scope:** The `poker-core/engine` submodule's probability and equity
  calculator: card/deck types, the hand evaluator, the seeded RNG and
  without-replacement sampler, the exact enumerator, the Monte-Carlo estimator,
  and the equity result type. This is the engine's "calculator" slice.
- **Contract source:** `docs/superpowers/specs/2026-10-05-poker-cli-design.md`
  (§2.5 engine boundary, §6.3 `prob`→`equity`) and
  `docs/superpowers/specs/2026-10-10-poker-server-design.md` (§3.1 `equity`
  seam). Where this document and those specs disagree, the older spec wins until
  it is deliberately revised (this document *is* that revision for §6.3, §5.4,
  §5.7, and §8.3).
- **Math source:** `docs/poker_probabilities.md` is the authority for the equity
  definition, the share function, the standard-error formula, and the
  scenario-count formula.
- **Dependencies:** none beyond `rand` and `rand_chacha`. `poker-core/engine` is
  pure — no I/O, no clock, no sockets, no `seq`, no persistence.
- **Language:** Rust. Part of the `poker-core` crate in the shared workspace.

---

## 1. Purpose, audience, and success criteria

`poker-core/engine` computes **equity** — a hand's expected share of the pot at
showdown against `n` opponents, given the hero's hole cards, the known board,
and (optionally) opponents' unknown hands. It powers the CLI's `equity` command
and, later, anything else that needs a fair, reproducible showdown probability.

**Success criteria**

- `equity(input, params)` returns equity (pot share in `[0,1]`), its standard
  error, and the win/tie/lose outcome frequencies, in under ~100 ms for the
  default run on a laptop.
- Given the same `(input, seed, iterations)`, the result is byte-identical.
- The default Monte-Carlo run targets a half-width `ε = 0.005` (≈ 40,000
  iterations), with the standard error always reported so precision is honest.
- `--exact` enumerates every scenario with zero simulation error, or fails
  loudly with the scenario count when enumeration is too large.
- The hand evaluator is correct and fast, verified by differential testing
  against a naive reference and a hand-checked fixture set.

**Audience**

| Audience | Needs |
|---|---|
| Human player | Readable "equity 41.32%" with win/tie/lose percentages. |
| AI agent | Stable versioned JSON (`poker.equity/1`) with equity, se, win/tie/lose as fractions. |
| Future engine modules | `Card`/`Deck` types and the `HandEvaluator` trait as shared infrastructure. |

---

## 2. Module layout and boundaries

```
poker-core/
  src/lib.rs          # re-exports: poker_core::engine::{equity, Equity, Card, ...}
  src/engine/
    mod.rs            # pub use of the pieces below
    card.rs           # Card, Rank, Suit, Deck
    evaluator.rs      # HandEvaluator trait + CactusKev (production) + Naive (test oracle)
    rng.rs            # seed parsing, ChaCha12, without-replacement sampler
    equity.rs         # Equity input/output types, exact enumerator, Monte-Carlo estimator
    error.rs          # EngineError
```

- **In scope:** `card.rs`, `evaluator.rs`, `rng.rs`, `equity.rs`, `error.rs`.
- **Out of scope (separate spec):** the hand state machine, betting legality and
  `legal_actions`, min-raise bounds, and showdown adjudication — the server
  spec §3.1 seams `new_hand`, `legal_actions`, `apply`, `auto_advance`, `step`.
  `card.rs` and `evaluator.rs` are *shared infrastructure* that future spec will
  consume; this spec defines them but does not build their consumers.
- `equity` is consumed **client-side** (the CLI links `poker-core`); the server
  never computes it (server spec §5.7).

---

## 3. Card and deck types

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Suit { Club, Diamond, Heart, Spade }

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Rank(u8);            // 2..=14 (14 = Ace)

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Card { rank: Rank, suit: Suit }
```

- `Card::parse("Ah")` / `Card::display()` round-trip the CLI wire notation
  (`2c`…`As`, with `T` for ten — `Ah`, `Kd`, `7c`, `Ts`).
- `Card::index() -> u8` is the canonical `0..52` index used for arrays/tables.
- `Deck::standard() -> [Card; 52]`.
- `Card::to_ck() -> u32` produces the Cactus Kev bit-encoding (§4); it is an
  *evaluator* detail confined to `evaluator.rs` and never leaks into the equity
  API or wire types.

---

## 4. Hand evaluator

```rust
trait HandEvaluator {
    /// Best 5-of-7 rank; LARGER = STRONGER; equal ranks == true showdown tie.
    fn rank_seven(&self, cards: &[Card; 7]) -> HandRank;   // HandRank(u16)
}
```

- **`CactusKev` (production).** The Cactus Kev bit-trick 5-card evaluator plus a
  21-subset reduction for 7-card hands: each card is one 32-bit integer encoding
  rank (bits 8–11), suit bit (bits 12–15), 13 straight bits (16–28), and a prime
  (low byte); a flush/straight are found by bit-masking, and everything else via
  the unique prime product through a perfect-hash table. Reference:
  <http://suffe.cool/poker/evaluator.html>. Tables are precomputed once via
  `std::sync::OnceLock` (the 7,462-entry `values` table, plus the small
  straight/flush tables and perfect hash).
- **`Naive` (test oracle).** Builds the best 5-card hand by group-and-sort
  logic. Lives behind `#[cfg(test)]` (or a feature flag); never in the hot path.
- **Rank contract:** `larger == stronger`, `equal == tie` — exactly what the
  probabilities doc §5 requires. Internally the canonical Cactus Kev table order
  is normalized to this contract; consumers only ever see `HandRank`.

---

## 5. RNG and without-replacement sampler

```rust
// rng.rs
fn seed_from_hex(s: &str) -> Result<u64, EngineError>;
type Rng = rand_chacha::ChaCha12Rng;

fn sample_without_replacement(rng: &mut Rng, pool: &[Card], m: usize) -> Vec<Card>;
```

- **RNG is ChaCha12** (`rand_chacha`), seeded via `seed_from_u64`. The `--seed`
  HEX string parses to a `u64`; absent/empty → derive from `OsRng` and echo the
  derived seed in the result so the run stays reproducible.
- **Sampler** is a **partial Fisher–Yates** over a copy of the unseen pool,
  returning the first `m` cards (probabilities doc §9). The pool is the full
  deck minus the dead cards (hero hole cards ∪ board); `m = 2·opponents + k`.
- **Determinism is version-scoped:** same `(input, seed, iterations)` →
  byte-identical result within schema `poker.equity/1`. Changing the RNG, the
  sampler, or the evaluator's rank mapping is a breaking change that bumps the
  schema version.
- **Single-threaded in v1.** The estimator core is a pure function over a slice
  of trials so deterministic parallel chunking (fixed chunk count, per-chunk
  sub-seeds, fixed-order reduce) can be added later as a non-breaking
  `--jobs N` flag.

---

## 6. Equity algorithm and public API

```rust
// equity.rs
pub struct EquityInput {
    pub hero: [Card; 2],
    pub board: Vec<Card>,       // exactly 0, 3, 4, or 5 cards
    pub opponents: usize,       // n >= 1
    // reserved: per-opponent range weights (seam only; ignored in v1)
}

pub enum EquityMode {
    MonteCarlo { iterations: usize },   // default 40_000
    Exact,
}

pub struct EquityParams { pub seed: u64, pub mode: EquityMode }

pub struct Equity {
    pub mode: EquityMode,
    pub equity: f64,       // expected pot share, [0,1]
    pub se: f64,           // standard error (0.0 in exact mode)
    pub win: f64, pub tie: f64, pub lose: f64,   // fractions; sum ≈ 1
    pub count: usize,      // scenarios evaluated: N in MC, |Ω| in exact
    pub opponents: usize,
    pub seed: u64,
}

pub fn equity(input: EquityInput, params: EquityParams) -> Result<Equity, EngineError>;
```

**Share function** (probabilities doc §5). Per scenario `ω`: `r_H` = hero rank,
`r_max` = max opponent rank, `t` = # opponents tying at `r_max`:

- `r_H > r_max` → share `1`, count a **win**
- `r_H == r_max` → share `1/(t+1)`, count a **tie**
- `r_H < r_max` → share `0`, count a **lose**

**Monte-Carlo path** (doc §9). `dead = hero ∪ board`, `Ucards = deck − dead`,
`k = 5 − board.len()`, `m = 2n + k`, require `m ≤ |Ucards|`. Loop `N` times:
sample `m` cards, deal 2 per opponent plus the runout, rank hero and each
opponent, accumulate `(sum, sum_sq, wins, ties, losses)`. Then `e = sum/N`,
`var = (sum_sq − N·e²)/(N−1)` (clamped `≥ 0`), `se = √(var/N)`. Default
`N = 40_000` (half-width `ε ≈ 0.005` per doc §6).

**Exact path.** Compute `|Ω|` via doc §4: `|Ω| = (∏ⱼ C(U−2j, 2)) · C(U−2n, k)`.
If `|Ω| > 1_100_000` → `EngineError::TooManyScenarios { scenarios }`. Otherwise
enumerate all scenarios lexicographically with a combinations iterator over card
indices (deterministic, no RNG), apply the share function, and set `se = 0`.

The 1.1M threshold admits river (990), turn (45,540), and heads-up flop
(1,070,190); it excludes multiway flop and all preflop.

---

## 7. Errors

```rust
enum EngineError {
    InvalidCardString(String),
    InvalidBoardSize(usize),          // not 0/3/4/5
    DuplicateCard(Card),
    NoOpponents,                      // n == 0
    NotEnoughCards { need: usize, have: usize },
    InvalidIterations(usize),         // N < 2 (SE needs N > 1)
    InvalidSeed(String),
    TooManyScenarios { scenarios: u64 },
}
```

These are surfaced client-side (the CLI links `poker-core`). Mapping:

- `InvalidCardString`, `InvalidBoardSize`, `DuplicateCard`, `NoOpponents`,
  `NotEnoughCards`, `InvalidIterations`, `InvalidSeed` → `usage` (exit 2).
- `TooManyScenarios` → `error.code = "too_many_scenarios"`, exit **15** (new).

---

## 8. Output schema — `poker.equity/1`

```json
{
  "schema": "poker.equity/1",
  "mode": "monte-carlo",
  "iterations": 40000,
  "opponents": 2,
  "seed": "0123456789abcdef",
  "equity": 0.4132,
  "se": 0.0019,
  "win": 0.4110, "tie": 0.0044, "lose": 0.5846
}
```

- Exact mode emits `"scenarios": <|Ω|>` instead of `"iterations"`, and `"se": 0`.
  The Rust `Equity.count` field is rendered as `iterations` in Monte-Carlo mode
  and `scenarios` in exact mode.
- `equity`, `win`, `tie`, `lose` are fractions in `[0,1]`; `win + tie + lose ≈ 1`.
- `seed` is always present (the *effective* seed — user-supplied or OS-derived).
- Human rendering prints percentages (`equity 41.32%`, `win 41.10% / tie 0.44%
  / lose 58.46%`) and shows `seed` only when `--seed` was explicitly passed.

---

## 9. Out of scope for v1

- Hand state machine, betting legality, `legal_actions`, min-raise bounds,
  showdown adjudication (separate engine spec).
- Range weighting beyond the reserved seam (probabilities doc §7) — uniform
  "fair" mode only; weighted enumeration and rejection sampling are deferred.
- Pot odds and call-EV helpers (doc §8).
- Parallel Monte-Carlo (`--jobs`); side pots and tournament/ICM payoff models.

---

## 10. Testing strategy

- **Differential:** `CactusKev` vs `Naive` — exhaustively over all
  `C(52,5) = 2,598,960` five-card hands, and over many random 7-card sets
  (21-subset max). Plus a hand-verified fixture list covering every hand class,
  the A-2-3-4-5 wheel, kicker edges, and aces-full-beats-kings-full.
- **`|Ω|` formula:** unit-test against the doc's checked values (1,070,190
  heads-up flop; 2,097,572,400 heads-up preflop).
- **MC vs exact cross-check:** exact turn heads-up (45,540) vs MC with large
  `N`; assert agreement within a few SE.
- **Threshold:** exact heads-up flop (1,070,190) succeeds; exact 2-opp flop
  fails with `too_many_scenarios` carrying the correct `|Ω|`.
- **Determinism:** same `(input, seed, N)` → identical bytes; absent seed →
  seed echoed and reproducible.
- **Invariants:** `equity ∈ [0,1]`, `win+tie+lose ≈ 1`, `se ≥ 0`.
- **Golden JSON** pinning `poker.equity/1`.
- **Well-known equities** as coarse sanity checks (e.g. AA vs KK, AKs vs a
  random hand) within tolerance.
- **Errors:** invalid board size, duplicate cards, `n = 0`, `N < 2`, bad seed,
  bad card string each map to the specified error.

---

## 11. Dependent spec changes (revise alongside this spec)

1. **CLI spec §4.3 + §6.3:** rename the `prob` command to `equity`; replace
   `poker.prob/1` with `poker.equity/1` and the shape in §8 above.
2. **CLI spec §8.3:** add `too_many_scenarios` (exit 15).
3. **Server spec §5.4 + §5.7:** rename the `prob` references to `equity` (the
   `equity` seam itself is unchanged).

---

## 12. Deliverables

1. This design spec.
2. An implementation plan (via the writing-plans skill) for `poker-core/engine`.
3. The CLI/server spec revisions listed in §11.
