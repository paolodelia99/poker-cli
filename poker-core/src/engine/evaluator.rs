use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

use super::card::RANK_PRIMES;
use super::Card;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HandRank(pub u16);

pub trait HandEvaluator {
    fn rank_five(&self, cards: &[Card; 5]) -> HandRank;

    fn rank_seven(&self, cards: &[Card; 7]) -> HandRank {
        let mut best = HandRank(0);
        for a in 0..3 {
            for b in (a + 1)..4 {
                for c in (b + 1)..5 {
                    for d in (c + 1)..6 {
                        for e in (d + 1)..7 {
                            best =
                                best.max(self.rank_five(&[
                                    cards[a], cards[b], cards[c], cards[d], cards[e],
                                ]));
                        }
                    }
                }
            }
        }
        best
    }
}

/// Cactus Kev's bit-trick 5-card evaluator.
///
/// The five 32-bit [`Card::to_ck`] encodings are OR'ed and shifted `>> 16` to
/// obtain a rank bitset. Flushes and unique-rank hands (high card and
/// straights) resolve through two 8192-entry rank-bitset tables; every other
/// hand resolves through a perfect-hash table keyed by the product of the five
/// rank primes. The lookup tables are built once in a [`OnceLock`].
pub struct CactusKev;

impl HandEvaluator for CactusKev {
    fn rank_five(&self, cards: &[Card; 5]) -> HandRank {
        let c = [
            cards[0].to_ck(),
            cards[1].to_ck(),
            cards[2].to_ck(),
            cards[3].to_ck(),
            cards[4].to_ck(),
        ];

        let tables = TABLES.get_or_init(build_tables);

        let q = ((c[0] | c[1] | c[2] | c[3] | c[4]) >> 16) as usize;
        if (c[0] & c[1] & c[2] & c[3] & c[4] & 0xF000) != 0 {
            return tables.flushes[q];
        }
        if let Some(rank) = tables.unique5[q] {
            return rank;
        }
        let product = (c[0] & 0xFF) * (c[1] & 0xFF) * (c[2] & 0xFF) * (c[3] & 0xFF) * (c[4] & 0xFF);
        tables.products[&product]
    }
}

struct Tables {
    /// Rank-bitset lookup for flush and straight-flush hands.
    flushes: [HandRank; 8192],
    /// Rank-bitset lookup for non-flush hands with five distinct ranks
    /// (high card and straights); `None` for rank patterns with a duplicate.
    unique5: [Option<HandRank>; 8192],
    /// Prime-product lookup for paired, tripped, and quadded hands.
    products: HashMap<u32, HandRank>,
}

static TABLES: OnceLock<Tables> = OnceLock::new();

/// Test-only convenience used by the [`Naive`] oracle: folds five [`Card`]s
/// into a rank histogram and flush flag, then delegates to [`classify_five`].
#[cfg(test)]
fn five_card_value(cards: &[Card; 5]) -> HandRank {
    let mut counts = [0u8; 13];
    for card in cards {
        counts[(card.rank.0 - 2) as usize] += 1;
    }
    let flush = cards.iter().all(|card| card.suit == cards[0].suit);
    classify_five(&counts, flush)
}

/// Canonical `larger == stronger` five-card classification shared by the
/// test-only [`Naive`] oracle and by [`build_tables`], so the bit-trick
/// evaluator and the oracle are order-isomorphic by construction.
fn classify_five(counts: &[u8; 13], flush: bool) -> HandRank {
    let straight = straight_high(counts);

    let mut quads = None;
    let mut trips = [0u8; 2];
    let mut trip_count = 0;
    let mut pairs = [0u8; 2];
    let mut pair_count = 0;
    let mut singles = [0u8; 5];
    let mut single_count = 0;
    for rank in (0..13).rev() {
        match counts[rank] {
            4 => quads = Some(rank as u8),
            3 => {
                trips[trip_count] = rank as u8;
                trip_count += 1;
            }
            2 => {
                pairs[pair_count] = rank as u8;
                pair_count += 1;
            }
            1 => {
                singles[single_count] = rank as u8;
                single_count += 1;
            }
            _ => {}
        }
    }

    if let Some(high) = straight.filter(|_| flush) {
        return HandRank((if high == 14 { 9 } else { 8 }) << 12 | high as u16);
    }
    if let Some(quad) = quads {
        return HandRank((7 << 12) | (quad as u16 * 13 + singles[0] as u16));
    }
    if trip_count > 0 && pair_count > 0 {
        return HandRank((6 << 12) | (trips[0] as u16 * 13 + pairs[0] as u16));
    }
    if flush {
        return HandRank(5 << 12 | combination_rank_desc(&singles, &[]));
    }
    if let Some(high) = straight {
        return HandRank(4 << 12 | high as u16);
    }
    if trip_count > 0 {
        return HandRank(
            (3 << 12)
                | (trips[0] as u16 * 220
                    + combination_rank_desc(&singles[..single_count], &[trips[0]])),
        );
    }
    if pair_count >= 2 {
        return HandRank(
            (2 << 12) | (pairs[0] as u16 * 169 + pairs[1] as u16 * 13 + singles[0] as u16),
        );
    }
    if pair_count == 1 {
        return HandRank(
            (1 << 12)
                | (pairs[0] as u16 * 220
                    + combination_rank_desc(&singles[..single_count], &[pairs[0]])),
        );
    }
    HandRank(combination_rank_desc(&singles[..single_count], &[]))
}

fn straight_high(counts: &[u8; 13]) -> Option<u8> {
    if counts.iter().filter(|&&count| count != 0).count() != 5 {
        return None;
    }
    for high in (6..=14).rev() {
        if (0..5).all(|offset| counts[(high - offset - 2) as usize] != 0) {
            return Some(high);
        }
    }
    if counts[12] != 0 && (0..4).all(|rank| counts[rank] != 0) {
        Some(5)
    } else {
        None
    }
}

fn combination_rank_desc(values: &[u8], excluded: &[u8]) -> u16 {
    let mut result = 0u16;
    let mut upper = 13u8;
    for (position, &value) in values.iter().enumerate() {
        let remaining = values.len() - position - 1;
        for candidate in 0..value {
            if candidate >= upper || excluded.contains(&candidate) {
                continue;
            }
            let available_below = (0..candidate)
                .filter(|rank| !excluded.contains(rank))
                .count() as u16;
            result += choose(available_below, remaining);
        }
        upper = value;
    }
    result
}

fn choose(n: u16, k: usize) -> u16 {
    if k == 0 {
        return 1;
    }
    if k > n as usize {
        return 0;
    }
    let mut result = 1u32;
    for i in 0..k {
        result = result * (n - i as u16) as u32 / (i as u32 + 1);
    }
    result as u16
}

/// Builds the Cactus Kev lookup tables by enumerating every distinct 5-rank
/// multiset and classifying it with the canonical comparator. The raw
/// classification values are then collapsed onto the reference `1..=7462`
/// strength scale, preserving order.
fn build_tables() -> Tables {
    let mut flushes = [HandRank(0); 8192];
    let mut unique5 = [None; 8192];
    let mut products = HashMap::new();
    let mut distinct = BTreeSet::new();

    // Enumerate every 5-rank multiset. The 13 five-of-a-kind tuples cannot be
    // formed from a 52-card deck and are skipped.
    for a in 0..13u8 {
        for b in a..13u8 {
            for c in b..13u8 {
                for d in c..13u8 {
                    for e in d..13u8 {
                        if a == b && b == c && c == d && d == e {
                            continue;
                        }

                        let ranks = [a, b, c, d, e];
                        let mut counts = [0u8; 13];
                        let mut q = 0u32;
                        let mut product = 1u32;
                        for rank in ranks {
                            counts[rank as usize] += 1;
                            q |= 1 << rank;
                            product *= RANK_PRIMES[rank as usize];
                        }

                        let unique = a < b && b < c && c < d && d < e;
                        if unique {
                            let flush_value = classify_five(&counts, true);
                            let unique_value = classify_five(&counts, false);
                            distinct.insert(flush_value);
                            distinct.insert(unique_value);
                            flushes[q as usize] = flush_value;
                            unique5[q as usize] = Some(unique_value);
                        } else {
                            let value = classify_five(&counts, false);
                            distinct.insert(value);
                            products.insert(product, value);
                        }
                    }
                }
            }
        }
    }

    debug_assert_eq!(distinct.len(), 7462);

    // Fixed bijection from raw classification values onto 1..=7462, keeping
    // `larger == stronger` (1 == weakest, 7462 == strongest).
    let mut normalize = BTreeMap::new();
    for (index, &value) in distinct.iter().enumerate() {
        normalize.insert(value, HandRank(index as u16 + 1));
    }

    for value in flushes.iter_mut() {
        if value.0 != 0 {
            let raw = *value;
            *value = normalize[&raw];
        }
    }
    for slot in unique5.iter_mut() {
        if let Some(value) = slot.as_mut() {
            let raw = *value;
            *value = normalize[&raw];
        }
    }
    for value in products.values_mut() {
        let raw = *value;
        *value = normalize[&raw];
    }

    Tables {
        flushes,
        unique5,
        products,
    }
}

#[cfg(test)]
pub struct Naive;

#[cfg(test)]
impl HandEvaluator for Naive {
    fn rank_five(&self, cards: &[Card; 5]) -> HandRank {
        five_card_value(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Deck;
    use super::*;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    use rand_chacha::ChaCha12Rng;

    fn r7(s: &str) -> HandRank {
        let cards: Vec<Card> = s
            .split_whitespace()
            .map(|card| Card::parse(card).unwrap())
            .collect();
        let cards: [Card; 7] = cards.try_into().unwrap();
        Naive.rank_seven(&cards)
    }

    fn random_hand(rng: &mut ChaCha12Rng) -> [Card; 7] {
        let mut deck = Deck::standard();
        deck.shuffle(rng);
        (&deck[..7]).try_into().unwrap()
    }

    #[test]
    fn rank_order_fixtures() {
        // royal flush > straight flush
        assert!(r7("As Ks Qs Js Ts 2d 3c") > r7("9h 8h 7h 6h 5h Ad Kc"));
        // straight-flush wheel < 6-high straight flush
        assert!(r7("Ah 2h 3h 4h 5h 9d Kc") < r7("2s 3s 4s 5s 6s Ad Kc"));
        // quads > full house
        assert!(r7("As Ah Ad Ac Ks 2d 3c") > r7("As Ah Ad Kh Kd 2c 3c"));
        // aces full > kings full
        assert!(r7("As Ah Ad Kh Kd 2c 3c") > r7("Ks Kh Kd Ah Ad 2c 3c"));
        // flush > straight
        assert!(r7("As Js 9s 7s 3s 2d 2c") > r7("6h 5d 4c 3s 2h 9c Kd"));
        // straight > trips
        assert!(r7("6h 5d 4c 3s 2h 9c Kd") > r7("As Ah Ad 7c 3c 4c 9c"));
        // wheel < 6-high straight (Review Focus #3)
        assert!(r7("Ah 2s 3d 4c 5h 9d Kc") < r7("2s 3d 4c 5h 6s 9d Kc"));
        // trips > two pair
        assert!(r7("As Ah Ad 7c 3c 4c 9c") > r7("As Ah Kd Kh 2c 3c 4c"));
        // two pair > pair
        assert!(r7("As Ah Kd Kh 2c 3c 4c") > r7("As Ah 2d 3c 4c 8c 9c"));
        // pair > high card
        assert!(r7("As Ah 2d 3c 4c 8c 9c") > r7("As Kh Qd Jc 9c 4c 2c"));
        // kicker edge: pair of aces, K kicker > Q kicker
        assert!(r7("As Ah Kd 3c 4c 5c 6c") > r7("As Ah Qd 3c 4c 5c 6c"));
        // ace-high kicker edge
        assert!(r7("As Kh Qd Jc 9c 4c 2c") > r7("As Kh Qd Jc 8c 4c 2c"));
        // tie: same rank class and kickers, different suits
        assert_eq!(r7("As Ah Kd 3c 4c 5c 6c"), r7("Ad Ac Ks 3h 4h 5h 6h"));
    }

    #[test]
    fn test_two_pair_kicker_does_not_tie() {
        let low = [
            Card::parse("2c").unwrap(),
            Card::parse("2d").unwrap(),
            Card::parse("3h").unwrap(),
            Card::parse("3s").unwrap(),
            Card::parse("4c").unwrap(),
        ];
        let high = [
            Card::parse("2c").unwrap(),
            Card::parse("2d").unwrap(),
            Card::parse("3h").unwrap(),
            Card::parse("3s").unwrap(),
            Card::parse("5c").unwrap(),
        ];
        assert!(Naive.rank_five(&low) < Naive.rank_five(&high));
    }

    #[test]
    fn test_seven_card_order_matches_naive_on_random() {
        let mut rng = ChaCha12Rng::seed_from_u64(0x5EED_CAFE);
        let mut results = Vec::with_capacity(100_000);
        for _ in 0..100_000 {
            let hand = random_hand(&mut rng);
            results.push((CactusKev.rank_seven(&hand), Naive.rank_seven(&hand)));
        }

        results.sort_by_key(|&(ck, _)| ck);
        let mut previous_ck = results[0].0;
        let mut previous_naive = results[0].1;
        for &(ck, naive) in &results[1..] {
            assert!(
                naive >= previous_naive,
                "naive ranks must be non-decreasing when sorted by CactusKev rank"
            );
            if ck == previous_ck {
                assert_eq!(
                    naive, previous_naive,
                    "equal CactusKev ranks must map to equal naive ranks"
                );
            }
            previous_ck = ck;
            previous_naive = naive;
        }
    }

    #[test]
    fn test_seven_card_reduction_is_max_of_subsets() {
        let mut rng = ChaCha12Rng::seed_from_u64(0xC0FFEE);
        for _ in 0..10_000 {
            let hand = random_hand(&mut rng);
            let mut best = HandRank(0);
            for a in 0..3 {
                for b in (a + 1)..4 {
                    for c in (b + 1)..5 {
                        for d in (c + 1)..6 {
                            for e in (d + 1)..7 {
                                best = best.max(
                                    CactusKev
                                        .rank_five(&[hand[a], hand[b], hand[c], hand[d], hand[e]]),
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(CactusKev.rank_seven(&hand), best);
        }
    }

    #[ignore]
    #[test]
    fn test_five_card_exhaustive_matches_naive() {
        let deck = Deck::standard();
        let mut results = Vec::with_capacity(2_598_960);
        for a in 0..52 {
            for b in (a + 1)..52 {
                for c in (b + 1)..52 {
                    for d in (c + 1)..52 {
                        for e in (d + 1)..52 {
                            let hand = [deck[a], deck[b], deck[c], deck[d], deck[e]];
                            results.push((CactusKev.rank_five(&hand), Naive.rank_five(&hand)));
                        }
                    }
                }
            }
        }

        results.sort_by_key(|&(ck, _)| ck);
        let mut previous_ck = results[0].0;
        let mut previous_naive = results[0].1;
        for &(ck, naive) in &results[1..] {
            if ck == previous_ck {
                assert_eq!(
                    naive, previous_naive,
                    "equal CactusKev ranks must map to equal naive ranks"
                );
            } else {
                assert!(
                    naive > previous_naive,
                    "naive ranks must strictly increase across distinct CactusKev ranks"
                );
            }
            previous_ck = ck;
            previous_naive = naive;
        }
    }
}
