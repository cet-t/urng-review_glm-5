use crate::rng::Rng;
use crate::sampler::Sampler;

/// Weighted sampler using Walker's Alias Method (O(1) sample, O(n) build).
///
/// Preferred over [`Bst64`](super::Bst64) when sampling repeatedly from the same weight distribution.
///
/// # Examples
///
/// ```
/// use urng::Sampler;
/// use urng::Alias64;
/// use urng::Mt1993764;
///
/// let mut rng = Mt1993764::new(1);
/// let mut sampler = Alias64::new(&mut rng, &[1.0f64, 2.0, 4.0, 8.0]);
/// assert!(sampler.sample() < 4);
///
/// // Update weights
/// sampler.weights(&[1.0, 1.0, 1.0, 1.0]);
/// assert!(sampler.sample() < 4);
/// ```
#[derive(Debug)]
pub struct Alias64<'a, R: Rng<Word = u64> + 'a> {
    rng: &'a mut R,
    prob: Vec<f64>,
    // u32 is sufficient (guarded by the build-time assertion); halves the
    // alias-table memory vs usize on 64-bit hosts (issue A7 note).
    alias: Vec<u32>,
}

impl<'a, R: Rng<Word = u64> + 'a> Alias64<'a, R> {
    /// Creates a new sampler with the given random number generator and weights.
    ///
    /// # Panics
    ///
    /// Panics if any weight is negative/NaN, the slice is empty, or the total
    /// weight is zero (see [`crate::sampler::validate_weights`]). Previously
    /// such input silently produced a uniform, unweighted distribution.
    pub fn new(rng: &'a mut R, weights: &[f64]) -> Self {
        let (prob, alias) = Self::build(weights);
        Self { rng, prob, alias }
    }

    fn build(weights: &[f64]) -> (Vec<f64>, Vec<u32>) {
        crate::sampler::validate_weights(weights);
        let n = weights.len();
        // Weights can never be numerous enough to overflow the u32 alias
        // table (the f64 prob table would need tens of GiB first); the
        // guard makes the narrowing below provably lossless.
        assert!(
            n <= u32::MAX as usize,
            "sampler weights: more than u32::MAX outcomes are not supported"
        );
        let total: f64 = weights.iter().sum();
        let scale = n as f64 / total;

        let mut prob = vec![0.0f64; n];
        let mut alias = vec![0u32; n];
        let mut small = Vec::with_capacity(n);
        let mut large = Vec::with_capacity(n);
        let mut p: Vec<f64> = weights.iter().map(|&w| w * scale).collect();

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

impl<'a, R: Rng<Word = u64> + 'a> Sampler<'a, R> for Alias64<'a, R> {
    fn weights(&mut self, weights: &[f64]) {
        let (prob, alias) = Self::build(weights);
        self.prob = prob;
        self.alias = alias;
    }

    fn sample(&mut self) -> usize {
        let n = self.prob.len();
        let i = self.rng.randi(0, n as i64 - 1) as usize;
        let u = self.rng.randf(0.0, 1.0);
        if u < self.prob[i] { i } else { self.alias[i] as usize }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prng::b64::Mt1993764;
    use crate::sampler::Sampler;

    #[test]
    fn alias64_works() {
        let mut rng = Mt1993764::new(1);
        let mut sampler = Alias64::new(&mut rng, &[0.1, 1.0, 10.0, 100.0]);
        let ones = (0..100_000).filter(|_| sampler.sample() == 1).count();
        assert_eq!(ones, 867);
    }

    /// Issue A7 regression: `Alias64` with an all-zero weight vector
    /// previously returned a silently-unweighted sampler (observed as a
    /// uniform `[0, 1, 0, 0]` probe); it now panics with a clear message.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias64_all_zero_weights_panic() {
        let mut rng = Mt1993764::new(2);
        let _ = Alias64::new(&mut rng, &[0.0f64, 0.0]);
    }

    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias64_negative_weight_panics() {
        let mut rng = Mt1993764::new(2);
        let _ = Alias64::new(&mut rng, &[1.0f64, -0.5]);
    }

    /// `weights()` runs the same validation as `new`.
    #[test]
    #[should_panic(expected = "sampler weights")]
    fn alias64_weights_update_validates() {
        let mut rng = Mt1993764::new(2);
        let mut sampler = Alias64::new(&mut rng, &[1.0f64, 1.0]);
        sampler.weights(&[]);
    }

    /// Zero weights on individual outcomes are fine (never sampled).
    #[test]
    fn alias64_zero_entries_never_selected() {
        let mut rng = Mt1993764::new(3);
        let mut sampler = Alias64::new(&mut rng, &[0.0f64, 1.0]);
        let hits0 = (0..2000).filter(|_| sampler.sample() == 0).count();
        assert_eq!(hits0, 0);
    }
}
