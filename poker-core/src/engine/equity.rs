use std::collections::HashSet;

use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;

use super::{
    sample_without_replacement, CactusKev, Card, Deck, EngineError, HandEvaluator, HandRank,
};

#[derive(Debug, Clone)]
pub struct EquityInput {
    pub hero: [Card; 2],
    pub board: Vec<Card>,
    pub opponents: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquityMode {
    MonteCarlo { iterations: usize },
    Exact,
}

#[derive(Debug, Clone, Copy)]
pub struct EquityParams {
    pub seed: u64,
    pub mode: EquityMode,
}

#[derive(Debug, PartialEq)]
pub struct Equity {
    pub mode: EquityMode,
    pub equity: f64,
    pub se: f64,
    pub win: f64,
    pub tie: f64,
    pub lose: f64,
    pub count: usize,
    pub opponents: usize,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Win,
    Tie,
    Lose,
}

fn share(hero: HandRank, opp: &[HandRank]) -> (f64, Outcome) {
    let max_opponent = *opp.iter().max().expect("at least one opponent");
    if hero > max_opponent {
        return (1.0, Outcome::Win);
    }
    if hero < max_opponent {
        return (0.0, Outcome::Lose);
    }

    let tying_opponents = opp.iter().filter(|&&rank| rank == max_opponent).count();
    (1.0 / (tying_opponents as f64 + 1.0), Outcome::Tie)
}

fn validate(input: &EquityInput, mode: EquityMode) -> Result<(Vec<Card>, usize), EngineError> {
    let mut dead = HashSet::with_capacity(input.hero.len() + input.board.len());
    for card in input.hero.iter().chain(input.board.iter()) {
        if !dead.insert(*card) {
            return Err(EngineError::DuplicateCard(*card));
        }
    }

    if !matches!(input.board.len(), 0 | 3 | 4 | 5) {
        return Err(EngineError::InvalidBoardSize(input.board.len()));
    }
    if input.opponents == 0 {
        return Err(EngineError::NoOpponents);
    }
    if let EquityMode::MonteCarlo { iterations } = mode {
        if iterations < 2 {
            return Err(EngineError::InvalidIterations(iterations));
        }
    }

    let pool: Vec<Card> = Deck::standard()
        .into_iter()
        .filter(|card| !dead.contains(card))
        .collect();
    let runout = 5 - input.board.len();
    let need = input
        .opponents
        .checked_mul(2)
        .and_then(|cards| cards.checked_add(runout))
        .unwrap_or(usize::MAX);
    if need > pool.len() {
        return Err(EngineError::NotEnoughCards {
            need,
            have: pool.len(),
        });
    }

    Ok((pool, need))
}

fn scenario_count(board_len: usize, opponents: usize) -> u64 {
    if board_len > 5 {
        return 0;
    }

    let unknown = 50 - board_len;
    let runout = 5 - board_len;
    if opponents > (unknown - runout) / 2 {
        return 0;
    }

    let mut scenarios = 1_u64;
    for opponent in 0..opponents {
        let available = unknown - 2 * opponent;
        scenarios = scenarios.saturating_mul(combination_count(available, 2));
    }
    scenarios.saturating_mul(combination_count(unknown - 2 * opponents, runout))
}

fn combination_count(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result = 1_u64;
    for i in 1..=k {
        result = result
            .checked_mul((n - k + i) as u64)
            .and_then(|value| value.checked_div(i as u64))
            .unwrap_or(u64::MAX);
    }
    result
}

fn rank_trial(
    hero: [Card; 2],
    board: &[Card],
    opponents: usize,
    sampled: &[Card],
) -> (f64, Outcome) {
    let evaluator = CactusKev;
    let runout_start = opponents * 2;
    let runout = &sampled[runout_start..];
    let make_seven = |hole: [Card; 2]| {
        let mut cards = [hole[0]; 7];
        cards[1] = hole[1];
        cards[2..2 + board.len()].copy_from_slice(board);
        cards[2 + board.len()..].copy_from_slice(runout);
        cards
    };

    let hero_rank = evaluator.rank_seven(&make_seven(hero));
    let opponent_ranks: Vec<HandRank> = (0..opponents)
        .map(|opponent| {
            let start = opponent * 2;
            evaluator.rank_seven(&make_seven([sampled[start], sampled[start + 1]]))
        })
        .collect();
    share(hero_rank, &opponent_ranks)
}

struct ExactStats {
    sum: f64,
    wins: usize,
    ties: usize,
    losses: usize,
}

fn enumerate_exact(input: &EquityInput, pool: &[Card], scenarios: u64) -> ExactStats {
    let mut stats = ExactStats {
        sum: 0.0,
        wins: 0,
        ties: 0,
        losses: 0,
    };
    let mut sampled = Vec::with_capacity(pool.len());
    enumerate_opponents(input, input.opponents, 0, pool, &mut sampled, &mut stats);
    debug_assert_eq!(stats.wins + stats.ties + stats.losses, scenarios as usize);
    stats
}

fn enumerate_opponents(
    input: &EquityInput,
    opponents: usize,
    opponent: usize,
    available: &[Card],
    sampled: &mut Vec<Card>,
    stats: &mut ExactStats,
) {
    if opponent == opponents {
        enumerate_runout(input, 5 - input.board.len(), 0, available, sampled, stats);
        return;
    }

    for first in 0..available.len() {
        for second in (first + 1)..available.len() {
            sampled.extend([available[first], available[second]]);
            let remaining: Vec<Card> = available
                .iter()
                .enumerate()
                .filter_map(|(index, &card)| (index != first && index != second).then_some(card))
                .collect();
            enumerate_opponents(input, opponents, opponent + 1, &remaining, sampled, stats);
            sampled.truncate(sampled.len() - 2);
        }
    }
}

fn enumerate_runout(
    input: &EquityInput,
    remaining: usize,
    start: usize,
    available: &[Card],
    sampled: &mut Vec<Card>,
    stats: &mut ExactStats,
) {
    if remaining == 0 {
        let (trial_share, outcome) = rank_trial(input.hero, &input.board, input.opponents, sampled);
        stats.sum += trial_share;
        match outcome {
            Outcome::Win => stats.wins += 1,
            Outcome::Tie => stats.ties += 1,
            Outcome::Lose => stats.losses += 1,
        }
        return;
    }
    if remaining > available.len() {
        return;
    }

    for index in start..=available.len() - remaining {
        sampled.push(available[index]);
        enumerate_runout(input, remaining - 1, index + 1, available, sampled, stats);
        sampled.pop();
    }
}

pub fn equity(input: EquityInput, params: EquityParams) -> Result<Equity, EngineError> {
    let (pool, cards_per_trial) = validate(&input, params.mode)?;

    if params.mode == EquityMode::Exact {
        let scenarios = scenario_count(input.board.len(), input.opponents);
        if scenarios > 1_100_000 {
            return Err(EngineError::TooManyScenarios { scenarios });
        }

        let stats = enumerate_exact(&input, &pool, scenarios);
        let count = scenarios as f64;
        return Ok(Equity {
            mode: params.mode,
            equity: stats.sum / count,
            se: 0.0,
            win: stats.wins as f64 / count,
            tie: stats.ties as f64 / count,
            lose: stats.losses as f64 / count,
            count: scenarios as usize,
            opponents: input.opponents,
            seed: params.seed,
        });
    }

    let iterations = match params.mode {
        EquityMode::MonteCarlo { iterations } => iterations,
        EquityMode::Exact => unreachable!(),
    };

    let mut rng = ChaCha12Rng::seed_from_u64(params.seed);
    let mut sum = 0.0;
    let mut sum_compensation = 0.0;
    let mut sum_sq = 0.0;
    let mut sum_sq_compensation = 0.0;
    let mut wins = 0usize;
    let mut ties = 0usize;
    let mut losses = 0usize;

    for _ in 0..iterations {
        let sampled = sample_without_replacement(&mut rng, &pool, cards_per_trial);
        let (trial_share, outcome) =
            rank_trial(input.hero, &input.board, input.opponents, &sampled);
        // Compensated summation keeps deterministic fixed-share cases (such as
        // a board royal flush) at their mathematically exact estimate instead
        // of accumulating avoidable rounding drift.
        let corrected_share = trial_share - sum_compensation;
        let next_sum = sum + corrected_share;
        sum_compensation = (next_sum - sum) - corrected_share;
        sum = next_sum;

        let trial_share_sq = trial_share * trial_share;
        let corrected_share_sq = trial_share_sq - sum_sq_compensation;
        let next_sum_sq = sum_sq + corrected_share_sq;
        sum_sq_compensation = (next_sum_sq - sum_sq) - corrected_share_sq;
        sum_sq = next_sum_sq;
        match outcome {
            Outcome::Win => wins += 1,
            Outcome::Tie => ties += 1,
            Outcome::Lose => losses += 1,
        }
    }

    let count = iterations as f64;
    let estimate = sum / count;
    let variance = ((sum_sq - count * estimate * estimate) / (count - 1.0)).max(0.0);
    let standard_error = (variance / count).sqrt();

    Ok(Equity {
        mode: params.mode,
        equity: estimate,
        se: standard_error,
        win: wins as f64 / count,
        tie: ties as f64 / count,
        lose: losses as f64 / count,
        count: iterations,
        opponents: input.opponents,
        seed: params.seed,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Card, EngineError, HandRank};
    use super::*;

    fn p(s: &str) -> Card {
        Card::parse(s).unwrap()
    }

    #[test]
    fn test_scenario_count() {
        assert_eq!(scenario_count(5, 1), 990);
        assert_eq!(scenario_count(4, 1), 45_540);
        assert_eq!(scenario_count(3, 1), 1_070_190);
        assert_eq!(scenario_count(0, 1), 2_097_572_400);
        assert_eq!(scenario_count(3, 2), 966_381_570);
    }

    #[test]
    fn test_well_known_equity_aces_preflop() {
        let e = equity(
            EquityInput {
                hero: [p("As"), p("Ah")],
                board: vec![],
                opponents: 1,
            },
            EquityParams {
                seed: 0,
                mode: EquityMode::MonteCarlo { iterations: 40_000 },
            },
        )
        .unwrap();
        assert!((0.80..=0.90).contains(&e.equity), "equity: {}", e.equity);
    }

    #[test]
    fn test_exact_nuts_is_certain_win() {
        let e = equity(
            EquityInput {
                hero: [p("As"), p("Ah")],
                board: vec![p("Ks"), p("Qs"), p("Js"), p("Ts"), p("2h")],
                opponents: 1,
            },
            EquityParams {
                seed: 0,
                mode: EquityMode::Exact,
            },
        )
        .unwrap();
        assert_eq!((e.equity, e.se, e.count), (1.0, 0.0, 990));
    }

    #[test]
    fn test_exact_threshold() {
        let flop = || EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![p("2c"), p("7d"), p("9h")],
            opponents: 1,
        };
        assert!(equity(
            flop(),
            EquityParams {
                seed: 0,
                mode: EquityMode::Exact
            }
        )
        .is_ok());
        let two = EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![p("2c"), p("7d"), p("9h")],
            opponents: 2,
        };
        assert!(matches!(
            equity(
                two,
                EquityParams {
                    seed: 0,
                    mode: EquityMode::Exact
                }
            ),
            Err(EngineError::TooManyScenarios {
                scenarios: 966_381_570
            })
        ));
        let pre = EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![],
            opponents: 1,
        };
        assert!(matches!(
            equity(
                pre,
                EquityParams {
                    seed: 0,
                    mode: EquityMode::Exact
                }
            ),
            Err(EngineError::TooManyScenarios {
                scenarios: 2_097_572_400
            })
        ));
    }

    #[test]
    fn test_mc_matches_exact_on_turn() {
        let input = EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![p("2c"), p("7d"), p("9h"), p("Qs")],
            opponents: 1,
        };
        let exact = equity(
            input.clone(),
            EquityParams {
                seed: 0,
                mode: EquityMode::Exact,
            },
        )
        .unwrap();
        let mc = equity(
            input,
            EquityParams {
                seed: 5,
                mode: EquityMode::MonteCarlo {
                    iterations: 2_000_000,
                },
            },
        )
        .unwrap();
        assert!((mc.equity - exact.equity).abs() <= 4.0 * mc.se);
    }

    #[test]
    fn test_share_function() {
        let h = |r| HandRank(r);
        assert_eq!(share(h(10), &[h(5), h(5)]), (1.0, Outcome::Win));
        assert_eq!(share(h(10), &[h(10), h(5)]), (0.5, Outcome::Tie));
        assert_eq!(
            share(h(10), &[h(10), h(10), h(5)]),
            (1.0 / 3.0, Outcome::Tie)
        );
        assert_eq!(share(h(5), &[h(10), h(10)]), (0.0, Outcome::Lose));
    }

    #[test]
    fn test_nuts_board_is_certain_win() {
        let e = equity(
            EquityInput {
                hero: [p("As"), p("Ah")],
                board: vec![p("Ks"), p("Qs"), p("Js"), p("Ts"), p("2h")],
                opponents: 1,
            },
            EquityParams {
                seed: 1,
                mode: EquityMode::MonteCarlo { iterations: 10_000 },
            },
        )
        .unwrap();
        assert_eq!(
            (e.equity, e.win, e.tie, e.lose, e.se),
            (1.0, 1.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn test_board_royal_flush_ties_everyone() {
        let board = vec![p("As"), p("Ks"), p("Qs"), p("Js"), p("Ts")];
        let e1 = equity(
            EquityInput {
                hero: [p("Ah"), p("Ad")],
                board: board.clone(),
                opponents: 1,
            },
            EquityParams {
                seed: 1,
                mode: EquityMode::MonteCarlo { iterations: 10_000 },
            },
        )
        .unwrap();
        assert_eq!((e1.equity, e1.tie), (0.5, 1.0));

        let e2 = equity(
            EquityInput {
                hero: [p("Ah"), p("Ad")],
                board,
                opponents: 2,
            },
            EquityParams {
                seed: 1,
                mode: EquityMode::MonteCarlo { iterations: 10_000 },
            },
        )
        .unwrap();
        assert_eq!((e2.equity, e2.tie), (1.0 / 3.0, 1.0));
    }

    #[test]
    fn test_invariants() {
        let inputs = [
            EquityInput {
                hero: [p("As"), p("Ah")],
                board: vec![],
                opponents: 1,
            },
            EquityInput {
                hero: [p("2c"), p("7d")],
                board: vec![p("9h"), p("Qs"), p("Kd")],
                opponents: 2,
            },
            EquityInput {
                hero: [p("Jh"), p("Jd")],
                board: vec![p("2c"), p("7s"), p("9h"), p("Qs")],
                opponents: 1,
            },
        ];

        for input in inputs {
            let e = equity(
                input,
                EquityParams {
                    seed: 123,
                    mode: EquityMode::MonteCarlo { iterations: 5_000 },
                },
            )
            .unwrap();
            for value in [e.equity, e.win, e.tie, e.lose] {
                assert!((0.0..=1.0).contains(&value));
            }
            assert!((e.win + e.tie + e.lose - 1.0).abs() <= 1e-9);
            assert!(e.se >= 0.0);
        }
    }

    #[test]
    fn test_determinism() {
        let input = EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![p("2c"), p("7d"), p("9h")],
            opponents: 1,
        };
        let params = EquityParams {
            seed: 99,
            mode: EquityMode::MonteCarlo { iterations: 10_000 },
        };
        assert_eq!(equity(input.clone(), params), equity(input, params));
    }

    #[test]
    fn test_validation_errors() {
        use EquityMode::MonteCarlo as mc;
        let ok = || EquityInput {
            hero: [p("As"), p("Ah")],
            board: vec![],
            opponents: 1,
        };
        let par = |n| EquityParams {
            seed: 1,
            mode: mc { iterations: n },
        };

        assert!(matches!(
            equity(
                EquityInput {
                    hero: [p("As"), p("Ah")],
                    board: vec![p("2c")],
                    opponents: 1,
                },
                par(100)
            ),
            Err(EngineError::InvalidBoardSize(1))
        ));
        assert!(matches!(
            equity(
                EquityInput {
                    hero: [p("As"), p("Ah")],
                    board: vec![p("As")],
                    opponents: 1,
                },
                par(100)
            ),
            Err(EngineError::DuplicateCard(_))
        ));
        assert!(matches!(
            equity(
                EquityInput {
                    hero: [p("As"), p("As")],
                    board: vec![],
                    opponents: 1,
                },
                par(100)
            ),
            Err(EngineError::DuplicateCard(_))
        ));
        assert!(matches!(
            equity(
                EquityInput {
                    hero: [p("As"), p("Ah")],
                    board: vec![],
                    opponents: 0,
                },
                par(100)
            ),
            Err(EngineError::NoOpponents)
        ));
        assert!(matches!(
            equity(ok(), par(1)),
            Err(EngineError::InvalidIterations(1))
        ));
        assert!(matches!(
            equity(
                EquityInput {
                    hero: [p("As"), p("Ah")],
                    board: vec![],
                    opponents: 30,
                },
                par(100)
            ),
            Err(EngineError::NotEnoughCards { .. })
        ));
    }
}
