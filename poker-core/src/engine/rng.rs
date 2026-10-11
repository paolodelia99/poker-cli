use rand::Rng;
use rand_chacha::ChaCha12Rng;

use super::{Card, EngineError};

/// Parses a `--seed` value as an even-length, big-endian hex string into a
/// `u64`.
///
/// Empty and odd-length inputs are rejected before radix parsing so that a
/// seed always denotes a whole number of bytes; non-hex input is rejected by
/// the parser itself.
pub fn seed_from_hex(s: &str) -> Result<u64, EngineError> {
    let invalid = || EngineError::InvalidSeed(s.to_string());
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return Err(invalid());
    }
    u64::from_str_radix(s, 16).map_err(|_| invalid())
}

/// Draws `m` distinct cards from `pool` with a partial Fisher–Yates shuffle.
///
/// `pool` is copied, so its order is left untouched. The returned vector holds
/// at most one of each pool card and is truncated to `m`, or to `pool.len()`
/// when `m` exceeds it (the most distinct cards the pool can supply).
pub fn sample_without_replacement(rng: &mut ChaCha12Rng, pool: &[Card], m: usize) -> Vec<Card> {
    let n = pool.len();
    let m = m.min(n);
    let mut v = pool.to_vec();
    for i in 0..m {
        let j = rng.gen_range(i..n);
        v.swap(i, j);
    }
    v.truncate(m);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Card, Deck, EngineError};
    use rand::SeedableRng;
    use rand_chacha::ChaCha12Rng;

    #[test]
    fn test_seed_from_hex() {
        assert_eq!(
            seed_from_hex("0123456789abcdef").unwrap(),
            0x0123_4567_89ab_cdef
        );
        assert!(matches!(
            seed_from_hex(""),
            Err(EngineError::InvalidSeed(_))
        ));
        assert!(matches!(
            seed_from_hex("xyz"),
            Err(EngineError::InvalidSeed(_))
        ));
        // odd length
        assert!(matches!(
            seed_from_hex("abc"),
            Err(EngineError::InvalidSeed(_))
        ));
    }

    #[test]
    fn test_sample_deterministic_and_no_replacement() {
        let pool: Vec<Card> = Deck::standard().to_vec();
        let mut a = ChaCha12Rng::seed_from_u64(42);
        let mut b = ChaCha12Rng::seed_from_u64(42);
        let da = sample_without_replacement(&mut a, &pool, 7);
        let db = sample_without_replacement(&mut b, &pool, 7);
        assert_eq!(da, db); // deterministic
        assert_eq!(da.len(), 7); // exact length
                                 // unique
        let mut s = std::collections::HashSet::new();
        assert!(da.iter().all(|c| s.insert(*c)));
        // all drawn from the pool
        assert!(da.iter().all(|c| pool.contains(c)));
    }

    #[test]
    fn test_sample_extremes() {
        let pool: Vec<Card> = Deck::standard().to_vec();
        let mut rng = ChaCha12Rng::seed_from_u64(7);
        assert!(sample_without_replacement(&mut rng, &pool, 0).is_empty());
        assert_eq!(sample_without_replacement(&mut rng, &pool, 52).len(), 52);
    }
}
