//! Weighted random selection.

pub mod b32;
pub mod b64;

pub use b32::*;
pub use b64::*;

use crate::rng::{Rng, Word};

/// Weighted random sampling trait, generic over the generator's [`Word`].
///
/// Implementors provide O(1) or O(log n) weighted index selection.
/// Available implementations: [`crate::sampler::b32::Bst32`], [`crate::sampler::b32::Alias32`],
/// [`crate::sampler::b64::Bst64`], [`crate::sampler::b64::Alias64`].
///
/// # Examples
///
/// ```
/// use urng::{Sampler, Alias32, Mt19937};
///
/// let mut rng = Mt19937::new(1);
/// let mut sampler = Alias32::new(&mut rng, &[1.0f32, 9.0]);
/// assert!(sampler.sample() < 2);
/// ```
pub trait Sampler<'a, R: Rng + 'a> {
    /// Samples a random index based on the weights.
    ///
    /// # Panics
    ///
    /// Panics if the sampler was built from weights that [`Alias32`]/[`Bst32`]
    /// (and the 64-bit variants) reject at build time — see
    /// [`validate_weights`].
    fn sample(&mut self) -> usize;
    /// Updates the weights of the sampler.
    ///
    /// # Panics
    ///
    /// Applies the same validation as [`Alias32::new`] /
    /// [`Bst32::new`] etc. (see [`validate_weights`]).
    fn weights(&mut self, weights: &[<R::Word as Word>::Float]);
}

/// Issue A7 input validation for all weighted samplers.
///
/// The previous samplers silently degenerated on invalid input: a negative or
/// NaN weight, or an all-zero weight vector, made the range scaling collapse
/// to NaN and the builders fell back to an *equal, unweighted* distribution
/// (observed: `Alias32` with `[0, 0, 0, 0]` returned a uniform index). Also,
/// an empty weight slice left a sampler whose `sample()` could only panic
/// deep inside range mapping with an unrelated message.
///
/// Valid input (documented contract):
/// - at least one weight;
/// - every weight finite and `>= 0` (individual zero weights are fine);
/// - total weight `> 0`.
///
/// Violations panic with a `sampler weights:`-prefixed message in both debug
/// and release builds, consistent with the `randi`/`to_randi` span contract
/// (issue A5).
pub(crate) fn validate_weights<F>(weights: &[F])
where
    F: Copy + Default + PartialOrd + std::ops::AddAssign + std::fmt::Debug,
{
    assert!(
        !weights.is_empty(),
        "sampler weights: empty distribution (cannot sample from zero outcomes)"
    );
    let mut total = F::default();
    for &w in weights {
        // `NaN >= 0` is false, so this also rejects NaN entries.
        assert!(
            w >= F::default(),
            "sampler weights: weight {w:?} is negative or NaN"
        );
        total += w;
    }
    assert!(
        total > F::default(),
        "sampler weights: total weight is zero (would silently yield an unweighted distribution)"
    );
}
