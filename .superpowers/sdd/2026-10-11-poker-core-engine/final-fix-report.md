# Final Fix Report: Engine Review Findings

## Status

Implemented in the fix wave described by the review. The commit subject is:

`test(engine): add spec-10 equity sanity check; fmt; doc fixes`

## Changes

- Added `test_well_known_equity_aces_preflop` to the `equity.rs` test module.
  It evaluates `As Ah` preflop against one random opponent for exactly 40,000
  Monte-Carlo iterations with seed `0`, and asserts the deterministic equity is
  in `[0.80, 0.90]`.
- Ran `cargo fmt` over the workspace, including the required formatting fixes in
  `poker-core/src/engine/card.rs`.
- Corrected the `InvalidIterations` documentation to state that iterations must
  be at least 2 because standard error requires `N > 1`.
- Added the central rank contract documentation to both `HandRank` and
  `HandEvaluator`: larger rank means stronger hand; equal rank means a true
  showdown tie.

## Verification

```text
cargo fmt
```
Completed successfully with no output.

```text
cargo test -p poker-core
```
Passed: 21 tests, 0 failed, 1 ignored; public integration test passed (1/1),
and doc tests passed (0 tests).

```text
cargo fmt --check
```
Passed with no output.

```text
git diff --check
```
Passed with no output.

The 2,000,000-iteration Monte-Carlo cross-check remains unchanged.
No dependencies or public signatures were changed.
