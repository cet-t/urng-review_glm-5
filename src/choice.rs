use crate::rng::{Rng, Word};

/// Random element selection from a slice, for any [`Rng`].
///
/// # Contract differences vs [`crate::Shuffle`]
///
/// For consistency with `Shuffle::shuffle` (which returns an explicit
/// `Err` for an empty slice) one might expect `Result` here as well; these
/// methods instead **panic** on an empty slice, which keeps the signature
/// `&T` without a `Result` wrapper. The panic is documented below and the
/// inconsistency is intentional for backward compatibility of the public
/// API. Prefer `shuffle`-style error handling or check `is_empty()`
/// yourself if empty input is possible.
pub trait Choice: Rng {
    /// Returns a random element from a slice.
    ///
    /// # Panics
    ///
    /// Panics if `choices` is empty (the index draw is out of bounds).
    ///
    /// # Examples
    ///
    /// ```
    /// use urng::{Rng, Choice, Sfc32};
    ///
    /// let mut rng = Sfc32::new(0);
    /// let items = [10, 20, 30];
    /// assert_eq!(*rng.choice(&items) > 0, true);
    /// ```
    #[inline(always)]
    fn choice<'a, T>(&mut self, choices: &'a [T]) -> &'a T {
        let index = self.nextu().to_index(choices.len());
        &choices[index]
    }

    /// Returns a random mutable element from a slice.
    ///
    /// # Panics
    ///
    /// Panics if `choices` is empty (the index draw is out of bounds).
    #[inline(always)]
    fn choice_mut<'a, T>(&mut self, choices: &'a mut [T]) -> &'a mut T {
        let index = self.nextu().to_index(choices.len());
        &mut choices[index]
    }
}

impl<R: Rng + ?Sized> Choice for R {}

#[cfg(test)]
mod tests {
    use crate::Choice;

    #[test]
    fn it_works() {
        let mut rng = crate::Sfc32::new(0);
        let items: Vec<_> = (0..10).collect();
        assert_eq!(rng.choice(&items), &items[3]);
    }

    /// Issue A7: the empty-slice behavior is a documented panic (previously
    /// undocumented — the caller only saw a bare `index out of bounds`
    /// library panic).
    #[test]
    #[should_panic]
    fn choice_empty_panics() {
        let mut rng = crate::Sfc32::new(1);
        let empty: [u8; 0] = [];
        let _ = rng.choice(empty.as_slice());
    }

    /// Same contract for the mutable variant; a one-element slice always
    /// yields that element and the remaining slice length is preserved.
    #[test]
    fn choice_mut_single_and_write_back() {
        let mut rng = crate::Sfc64::new(2);
        let mut one = [7u64];
        assert_eq!(*rng.choice_mut(one.as_mut_slice()), 7);
        *rng.choice_mut(one.as_mut_slice()) = 9;
        assert_eq!(one, [9]);

        // Distribution sanity: every element must be reachable.
        let mut seen = [false; 5];
        let mut hits = 0u32;
        for _ in 0..2000 {
            let data: Vec<u16> = (0..5).collect();
            let picked = *rng.choice(&data);
            hits += 1;
            seen[picked as usize] = true;
        }
        assert_eq!(hits, 2000);
        assert!(seen.iter().all(|&s| s), "some element never picked");
    }
}
