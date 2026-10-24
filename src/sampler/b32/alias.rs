use crate::rng::Rng;
use crate::sampler::Sampler;

/// Weighted sampler using Walker's Alias Method (O(1) sample, O(n) build).
///
/// Preferred over [`Bst32`](super::Bst32) when sampling repeatedly from the same weight distribution.
///
/// # Examples
///
/// ```
/// use urng::Sampler;
/// use urng::Alias32;
/// use urng::Mt19937;
///
/// let mut rng = Mt19937::new(1);
/// let mut sampler = Alias32::new(&mut rng, &[1.0f32, 9.0]);
/// assert!(sampler.sample() < 2);
///
/// // Update weights
/// sampler.weights(&[5.0, 5.0]);
/// assert!(sampler.sample() < 2);
/// ```
#[derive(Debug)]
pub struct Alias32<'a, R: Rng<Word = u32> + 'a> {
    rng: &'a mut R,
    prob: Vec<f32>,
    // u32 is sufficient (guarded by the build-time assertion); halves the
    // alias-table memory vs usize on 64-bit hosts (issue A7 note).
    alias: Vec<u32>,
}

impl<'a, R: Rng<Word = u32> + 'a> Alias32<'a, R> {
    /// Creates a new sampler with the given random number generator and weights.
    ///
    /// # Panics
    ///
    /// Panics if any weight is negative/NaN, the slice is empty, or the total
    /// weight is zero (see [`crate::sampler::validate_weights`]). Previously
    /// such input silently produced a uniform, unweighted distribution.
    pub fn new(rng: &'a mut R, weights: &[f32]) -> Self {
        let (prob, alias) = Self::build(weights);
        Self { rng, prob, alias }
    }

    fn build(weights: &[f32]) -> (Vec<f32>, Vec<u32>) {
        crate::sampler::validate_weights(weights);
        let n = weights.len();
        // Weights can never be numerous enough to overflow the u32 alias
        // table (the f64/f32 prob table would need tens of GiB first); the
        // guard makes the narrowing below provably lossless.
        assert!(
            n <= u32::MAX as usize,
            "sampler weights: more than u32::MAX outcomes are not supported"
        );
        let total: f32 = weights.iter().sum();
        let scale = n as f32 / total;

        let mut prob = vec![0.0f32; n];
        let mut alias = vec![0u32; n];
        let mut small = Vec::with_capacity(n);
        let mut large = Vec::with_capacity(n);
        let mut p: Vec<f32> = weights.iter().map(|&w| w * scale).collect();

        for (i, &pi) in p.iter().enumerate() {
            if pi < 1.0 {
                small.push(i);
            } else {
                large.push(i);
            }
        }

        while let (Some(s), Some(l)) = (small.pop(), large.last().copied()) {
            prob[s] = p[s];
            alias[s] = l as u32;
            p[l] -= 1.0 - p[s];
            if p[l] < 1.0 {
                large.pop();
                small.push(l);
            }
        }
        for i in large {
            prob[i] = 1.0;
        }
        for i in small {
            prob[i] = 1.0;
        }

        (prob, alias)
    }
}

impl<'a, R: Rng<Word = u32> + 'a> Sampler<'a, R> for Alias32<'a, R> {
    fn weights(&mut self, weights: &[f32]) {
        let (prob, alias) = Self::build(weights);
        self.prob = prob;
        self.alias = alias;
    }

    fn sample(&mut self) -> usize {
        let n = self.prob.len();
        let i = self.rng.randi(0, n as i32 - 1) as usize;
        let u = self.rng.randf(0.0, 1.0);
        if u < self.prob[i] { i } else { self.alias[i] as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prng::b32::Mt19937;
    use crate::sampler::Sampler;

    #[test]
    fn alias32_works() {
        let mut rng = Mt19937::new(1);
        let mut sampler = Alias32::new(&mut rng, &[1.0f32, 9.0f32]);
        let ones = (0..100).filter(|_| sampler.sample() == 1).count();
        assert_eq!(ones, 91);
    }

    /// Issue A7: all-zero weights previously collapsed to NaN scaling and
    /// silently produced an equal (unweighted) distribution. Now they panic.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias32_all_zero_weights_panic() {
        let mut rng = Mt19937::new(2);
        let _ = Alias32::new(&mut rng, &[0.0f32, 0.0, 0.0, 0.0]);
    }

    /// Negative weights previously degenerated the same way (NaN scale).
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias32_negative_weight_panics() {
        let mut rng = Mt19937::new(2);
        let _ = Alias32::new(&mut rng, &[1.0f32, -1.0]);
    }

    /// Weight updates run the same validation as construction.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias32_weights_update_validates() {
        let mut rng = Mt19937::new(2);
        let mut sampler = Alias32::new(&mut rng, &[1.0f32, 9.0]);
        sampler.weights(&[0.0f32, 0.0]);
    }

    /// Empty weights previously left a sampler whose `sample()` failed with
    /// an unrelated range panic; it now fails at build time with a clear one.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias32_empty_weights_panic() {
        let mut rng = Mt19937::new(2);
        let _ = Alias32::new(&mut rng, &[]);
    }

    /// Individual zero weights are legal (never sampled): index 0 with
    /// weight 0 must never be selected.
    #[test]
    fn alias32_zero_entries_never_selected() {
        let mut rng = Mt19937::new(3);
        let mut sampler = Alias32::new(&mut rng, &[0.0f32, 1.0]);
        let hits0 = (0..2000).filter(|_| sampler.sample() == 0).count();
        assert_eq!(hits0, 0);
    }
}
