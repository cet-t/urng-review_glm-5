use crate::rng::Rng;
use crate::sampler::Sampler;

/// Weighted sampler using cumulative sums and binary search (O(log n) sample, O(n) build).
///
/// # Examples
///
/// ```
/// use urng::Sampler;
/// use urng::Bst32;
/// use urng::Mt19937;
///
/// let mut rng = Mt19937::new(1);
/// let mut sampler = Bst32::new(&mut rng, &[1.0f32, 9.0]);
/// assert!(sampler.sample() < 2);
/// ```
#[derive(Debug)]
pub struct Bst32<'a, R: Rng<Word = u32> + 'a> {
    rng: &'a mut R,
    cumulative: Vec<f32>,
}

impl<'a, R: Rng<Word = u32> + 'a> Bst32<'a, R> {
    /// Creates a new sampler with the given random number generator and weights.
    ///
    /// # Panics
    ///
    /// Panics if any weight is negative/NaN, the slice is empty, or the total
    /// weight is zero (see [`crate::sampler::validate_weights`]). Previously
    /// such input silently pinned `sample()` to index 0.
    pub fn new(rng: &'a mut R, weights: &[f32]) -> Self {
        Self {
            rng,
            cumulative: Self::build_cumulative(weights),
        }
    }

    fn build_cumulative(weights: &[f32]) -> Vec<f32> {
        crate::sampler::validate_weights(weights);
        let mut cumulative = Vec::with_capacity(weights.len());
        let mut sum = 0.0f32;
        for &w in weights {
            sum += w;
            cumulative.push(sum);
        }
        cumulative
    }
}

impl<'a, R: Rng<Word = u32> + 'a> Sampler<'a, R> for Bst32<'a, R> {
    fn weights(&mut self, weights: &[f32]) {
        self.cumulative = Self::build_cumulative(weights);
    }

    fn sample(&mut self) -> usize {
        let total = *self.cumulative.last().unwrap_or(&0.0);
        let r = self.rng.randf(0.0, total);
        match self
            .cumulative
            .binary_search_by(|&c| c.partial_cmp(&r).unwrap())
        {
            Ok(i) | Err(i) => i.min(self.cumulative.len() - 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prng::b32::Mt19937;
    use crate::sampler::Sampler;

    #[test]
    fn bst32_works() {
        let mut rng = Mt19937::new(1);
        let mut sampler = Bst32::new(&mut rng, &[1.0f32, 9.0f32]);
        let ones = (0..100).filter(|_| sampler.sample() == 1).count();
        assert_eq!(ones, 87);
    }

    /// Issue A7: an all-zero weight vector previously made `total == 0`, so
    /// `randf(0, 0)` returned 0 and `sample()` silently pinned to index 0.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn bst32_all_zero_weights_panic() {
        let mut rng = Mt19937::new(2);
        let _ = Bst32::new(&mut rng, &[0.0f32, 0.0]);
    }

    #[test]
    #[should_panic(expected = "sampler weights")]
    fn bst32_negative_weight_panics() {
        let mut rng = Mt19937::new(2);
        let _ = Bst32::new(&mut rng, &[1.0f32, -1.0, 1.0]);
    }

    /// `weights()` runs the same validation as `new`.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn bst32_weights_update_validates() {
        let mut rng = Mt19937::new(2);
        let mut sampler = Bst32::new(&mut rng, &[1.0f32, 1.0]);
        sampler.weights(&[0.0f32, 0.0, 0.0]);
    }

    /// Individual zero weights are legal (never sampled) and the cumulative
    /// array keeps its size: index 0 with weight 0 must never be selected.
    #[test]
    fn bst32_zero_entries_never_selected() {
        let mut rng = Mt19937::new(3);
        let mut sampler = Bst32::new(&mut rng, &[0.0f32, 1.0]);
        let hits0 = (0..2000).filter(|_| sampler.sample() == 0).count();
        assert_eq!(hits0, 0);
    }
}
