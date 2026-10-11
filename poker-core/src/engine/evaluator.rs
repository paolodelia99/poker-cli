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

#[cfg(test)]
pub struct Naive;

#[cfg(test)]
impl HandEvaluator for Naive {
    fn rank_five(&self, cards: &[Card; 5]) -> HandRank {
        let mut counts = [0u8; 13];
        for card in cards {
            counts[(card.rank.0 - 2) as usize] += 1;
        }

        let flush = cards.iter().all(|card| card.suit == cards[0].suit);
        let straight = straight_high(&counts);

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
            return HandRank(7 << 12 | quad as u16 * 13 + singles[0] as u16);
        }
        if trip_count > 0 && pair_count > 0 {
            return HandRank(6 << 12 | trips[0] as u16 * 13 + pairs[0] as u16);
        }
        if flush {
            return HandRank(5 << 12 | combination_rank_desc(&singles, &[]));
        }
        if let Some(high) = straight {
            return HandRank(4 << 12 | high as u16);
        }
        if trip_count > 0 {
            return HandRank(
                3 << 12
                    | trips[0] as u16 * 220
                    | combination_rank_desc(&singles[..single_count], &[trips[0]]),
            );
        }
        if pair_count >= 2 {
            return HandRank(
                2 << 12 | pairs[0] as u16 * 169 | pairs[1] as u16 * 13 | singles[0] as u16,
            );
        }
        if pair_count == 1 {
            return HandRank(
                1 << 12
                    | pairs[0] as u16 * 220
                    | combination_rank_desc(&singles[..single_count], &[pairs[0]]),
            );
        }
        HandRank(combination_rank_desc(&singles[..single_count], &[]))
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn r7(s: &str) -> HandRank {
        let cards: Vec<Card> = s
            .split_whitespace()
            .map(|card| Card::parse(card).unwrap())
            .collect();
        let cards: [Card; 7] = cards.try_into().unwrap();
        Naive.rank_seven(&cards)
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
}
