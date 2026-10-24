/// Error returned when attempting to shuffle an empty slice.
#[derive(Debug)]
pub struct SliceShuffleError(pub(crate) ());
impl std::fmt::Display for SliceShuffleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cannot shuffle an empty slice.")
    }
}
impl std::error::Error for SliceShuffleError {}

/// Result type for shuffle operations, failing on an empty slice.
pub type ShuffleResult<T> = std::result::Result<T, self::SliceShuffleError>;

/// An iterator that yields references to the elements of a slice in a shuffled order.
///
/// Returned by [`Shuffle::shuffled`].
pub struct ShuffledIterator<'a, T> {
    pub(crate) slice: &'a [T],
    pub(crate) indices: Vec<usize>,
    pub(crate) pos: usize,
}

impl<'a, T> Iterator for ShuffledIterator<'a, T> {
    type Item = &'a T;

    #[inline(always)]
    fn next(&mut self) -> Option<&'a T> {
        let index = *self.indices.get(self.pos)?;
        self.pos += 1;
        Some(&self.slice[index])
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.indices.len() - self.pos;
        (remaining, Some(remaining))
    }
}

use crate::rng::{Rng, Word};

/// Shuffles a slice in-place or returns a shuffled iterator, for any [`Rng`].
pub trait Shuffle: Rng {
    /// Shuffles `src` in place.
    ///
    /// Standard Fisher–Yates: at position `i` the swap partner is drawn
    /// from the not-yet-fixed suffix `[i, len)`, i.e.
    /// `j = i + nextu().to_index(len - i)`. (The previous version drew from
    /// the full `[0, len)` for every `i`, a biased shuffle  Ee.g. for
    /// `len == 3` the identity permutation appeared with probability ~0.22
    /// instead of 1/6 ≁E0.167, issue A3.)
    #[inline(always)]
    fn shuffle<T>(&mut self, src: &mut [T]) -> crate::ShuffleResult<()> {
        if src.is_empty() {
            Err(crate::SliceShuffleError(()))
        } else {
            let len = src.len();
            (0..len - 1).for_each(|i| {
                let j = i + self.nextu().to_index(len - i);
                src.swap(i, j);
            });
            Ok(())
        }
    }

    /// Returns an iterator over `src` in a shuffled order.
    #[inline(always)]
    fn shuffled<'a, T>(
        &mut self,
        src: &'a [T],
    ) -> crate::ShuffleResult<crate::ShuffledIterator<'a, T>> {
        if src.is_empty() {
            return Err(crate::SliceShuffleError(()));
        }
        let len = src.len();
        let mut indices: Vec<usize> = (0..len).collect();
        // Same corrected Fisher–Yates as [`Shuffle::shuffle`] (issue A3).
        (0..len - 1).for_each(|i| {
            let j = i + self.nextu().to_index(len - i);
            indices.swap(i, j);
        });
        Ok(crate::ShuffledIterator {
            slice: src,
            indices,
            pos: 0,
        })
    }
}

impl<R: Rng + ?Sized> Shuffle for R {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every element must appear exactly once (validity of the permutation,
    /// regardless of the distribution over permutations).
    fn assert_valid_permutation(len: usize, order: &[impl Copy + Into<usize>]) {
        let mut seen = vec![false; len];
        for &raw in order {
            let ix: usize = raw.into();
            assert!(ix < len, "index {ix} out of range for len {len}");
            assert!(!seen[ix], "index {ix} repeated");
            seen[ix] = true;
        }
    }

    #[test]
    fn shuffle_len2_first_stays_half_the_time() {
        // Unbiased FY gives P(v[0] stays 0) = 1/2 exactly.
        let mut rng = crate::Sfc32::new(1);
        let n = 100_000;
        let mut stays = 0u32;
        for _ in 0..n {
            let mut v = [0u8, 1];
            rng.shuffle(v.as_mut_slice()).unwrap();
            assert_valid_permutation(2, &v);
            stays += u32::from(v[0] == 0);
        }
        let p = stays as f64 / n as f64;
        assert!(
            (0.47..=0.53).contains(&p),
            "P(first stays) = {p}, expected ≁E0.5"
        );
    }

    #[test]
    fn shuffle_len3_permutation_census_is_uniform() {
        // Regression for issue A3: the biased implementation produced the
        // identity permutation for len == 3 with probability ≁E0.2211
        // (measured on the baseline); unbiased Fisher–Yates gives 1/6 ≁E
        // 0.1667 for every one of the 3! = 6 permutations.
        let mut rng = crate::Sfc32::new(2);
        let n = 600_000;
        let mut counts = [0u32; 6];
        for _ in 0..n {
            let mut v = [0u8, 1, 2];
            rng.shuffle(v.as_mut_slice()).unwrap();
            // Lexicographic rank of the 3-permutation (always 0..6): the
            // first digit is v[0], the second is v[1]'s rank among the two
            // remaining elements.
            let second = if v[1] > v[0] { v[1] - 1 } else { v[1] };
            let ix = 2 * usize::from(v[0]) + usize::from(second);
            counts[ix] += 1;
        }
        let expected = 1.0 / 6.0;
        for (ix, &c) in counts.iter().enumerate() {
            let p = c as f64 / n as f64;
            assert!(
                (expected / 1.15..=expected * 1.15).contains(&p),
                "permutation #{ix} ({}{}{}): p = {p:.5}, expected ≁E1/6",
                ix / 3,
                ix % 3,
                3 - ix / 3 - ix % 3
            );
        }
    }

    #[test]
    fn shuffled_iter_matches_shuffle_distribution() {
        // The `shuffled` path had the same bias (A3); a len-3 census must
        // also be uniform.
        let mut rng = crate::Sfc32::new(3);
        let n = 600_000;
        let mut counts = [0u32; 6];
        for _ in 0..n {
            let src = [0u8, 1, 2];
            let order: Vec<usize> = rng.shuffled(&src).unwrap().map(|&x| usize::from(x)).collect();
            assert_valid_permutation(3, &order);
            // Same lexicographic rank as in the shuffle census above.
            let second = if order[1] > order[0] { order[1] - 1 } else { order[1] };
            let ix = 2 * order[0] + second;
            counts[ix] += 1;
        }
        let expected = 1.0 / 6.0;
        for (ix, &c) in counts.iter().enumerate() {
            let p = c as f64 / n as f64;
            assert!(
                (expected / 1.15..=expected * 1.15).contains(&p),
                "permutation #{ix}: p = {p:.5}, expected ≁E1/6"
            );
        }
    }

    #[test]
    fn shuffle_edge_cases() {
        let mut rng = crate::Sfc32::new(4);
        // Empty slice is the documented error case.
        let mut empty: [u8; 0] = [];
        assert!(rng.shuffle(empty.as_mut_slice()).is_err());
        assert!(rng.shuffled(&empty).is_err());
        // A single element must be unchanged.
        let mut one = [7u8];
        rng.shuffle(one.as_mut_slice()).unwrap();
        assert_eq!(one, [7]);
        // Larger shuffles stay valid permutations.
        let mut big: Vec<usize> = (0..97).collect();
        rng.shuffle(big.as_mut_slice()).unwrap();
        assert_valid_permutation(97, &big);
    }
}
