/// An unordered pair of two distinct values
#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct UnorderedPair<T: PartialEq>(T, T);

impl<T: PartialEq> PartialEq for UnorderedPair<T> {
    fn eq(&self, other: &Self) -> bool {
        (self.0 == other.0 && self.1 == other.1) || (self.0 == other.1 && self.1 == other.0)
    }
}

impl<T: Eq> Eq for UnorderedPair<T> {}
impl<T: Ord + Eq> Ord for UnorderedPair<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_min = (&self.0).min(&self.1);
        let self_max = (&self.0).max(&self.1);

        let other_min = (&other.0).min(&other.1);
        let other_max = (&other.0).max(&other.1);

        match self_min.cmp(other_min) {
            std::cmp::Ordering::Equal => self_max.cmp(other_max),
            ord => return ord,
        }
    }
}

impl<T: PartialEq> UnorderedPair<T> {
    /// Creates a new UnorderedPair from two values without checking for uniqueness.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it doesn't enforce the invariant that
    /// the two values must be different. Use `UnorderedPair::new()` for safe construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// // Safe usage with distinct values
    /// let pair = unsafe { UnorderedPair::new_unchecked(1, 2) };
    /// assert_eq!(pair.first(), &1);
    /// assert_eq!(pair.second(), &2);
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// // Unsafe usage - violates invariant (but compiles)
    /// let pair = unsafe { UnorderedPair::new_unchecked(5, 5) };
    /// // This pair contains duplicate values, which violates the type's contract
    /// ```
    pub unsafe fn new_unchecked(a: T, b: T) -> Self {
        Self(a, b)
    }

    /// Creates a new UnorderedPair from two distinct values.
    /// Returns `Some(UnorderedPair)` if the values are different, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(1, 2);
    /// assert!(pair.is_some());
    /// let pair = pair.unwrap();
    /// assert_eq!(pair.first(), &1);
    /// assert_eq!(pair.second(), &2);
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(5, 5);
    /// assert!(pair.is_none()); // Same values are not allowed
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new("hello", "world");
    /// assert!(pair.is_some());
    /// let pair = pair.unwrap();
    /// assert!(pair.contains(&"hello"));
    /// assert!(pair.contains(&"world"));
    /// ```
    pub fn new(a: T, b: T) -> Option<Self> {
        if a != b { Some(Self(a, b)) } else { None }
    }

    /// Checks if the unordered pair contains a given value
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(10, 20).unwrap();
    /// assert!(pair.contains(&10));
    /// assert!(pair.contains(&20));
    /// assert!(!pair.contains(&30));
    /// ```
    pub fn contains(&self, x: &T) -> bool {
        self.0 == *x || self.1 == *x
    }

    /// Returns the first element of the unordered pair
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(5, 10).unwrap();
    /// assert_eq!(pair.first(), &5);
    /// ```
    pub fn first(&self) -> &T {
        &self.0
    }

    /// Returns the second element of the unordered pair
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(5, 10).unwrap();
    /// assert_eq!(pair.second(), &10);
    /// ```
    pub fn second(&self) -> &T {
        &self.1
    }

    /// Returns both elements as a tuple
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(7, 8).unwrap();
    /// assert_eq!(pair.as_tuple(), (&7, &8));
    /// ```
    pub fn as_tuple(&self) -> (&T, &T) {
        (&self.0, &self.1)
    }

    /// Returns both elements as a tuple of owned values
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(7, 8).unwrap();
    /// assert_eq!(pair.into_tuple(), (7, 8));
    /// ```
    pub fn into_tuple(self) -> (T, T) {
        (self.0, self.1)
    }

    pub fn into_arr(self) -> [T; 2] {
        [self.0, self.1]
    }

    /// Returns `true` if the unordered pair contains both given values
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_pair::UnorderedPair;
    ///
    /// let pair = UnorderedPair::new(1, 2).unwrap();
    /// assert!(pair.contains_both(&1, &2));
    /// assert!(pair.contains_both(&2, &1)); // Order doesn't matter
    /// assert!(!pair.contains_both(&1, &3));
    /// ```
    pub fn contains_both(&self, a: &T, b: &T) -> bool {
        self.contains(a) && self.contains(b)
    }
}

impl<T: PartialEq> TryFrom<(T, T)> for UnorderedPair<T> {
    type Error = &'static str;

    fn try_from(value: (T, T)) -> Result<Self, Self::Error> {
        if value.0 != value.1 {
            Ok(Self(value.0, value.1))
        } else {
            Err("UnorderedPair requires two distinct values")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_distinct_values() {
        let pair = UnorderedPair::new(1, 2);
        assert!(pair.is_some());
        let pair = pair.unwrap();
        assert_eq!(pair.first(), &1);
        assert_eq!(pair.second(), &2);
    }

    #[test]
    fn test_new_with_equal_values() {
        let pair = UnorderedPair::new(5, 5);
        assert!(pair.is_none());
    }

    #[test]
    fn test_from_tuple() {
        let pair: UnorderedPair<i32> = unsafe { UnorderedPair::new_unchecked(3, 4) };
        assert_eq!(pair, UnorderedPair(3, 4));
    }

    // Contains method tests
    #[test]
    fn test_contains_integers() {
        let pair = UnorderedPair::new(10, 20).unwrap();

        // Positive cases
        assert!(pair.contains(&10));
        assert!(pair.contains(&20));

        // Negative cases
        assert!(!pair.contains(&5));
        assert!(!pair.contains(&15));
        assert!(!pair.contains(&25));
    }

    #[test]
    fn test_contains_strings() {
        let pair = UnorderedPair::new("apple".to_string(), "banana".to_string()).unwrap();

        assert!(pair.contains(&"apple".to_string()));
        assert!(pair.contains(&"banana".to_string()));
        assert!(!pair.contains(&"cherry".to_string()));
    }

    // Test that unsafe constructor allows duplicates (but shouldn't be used that way)
    #[test]
    fn test_unsafe_constructor_allows_duplicates() {
        let pair = unsafe { UnorderedPair::new_unchecked(5, 5) };
        // This is allowed by the unsafe constructor but violates the type's invariant
        assert!(pair.contains(&5));
        assert_eq!(pair.first(), &5);
        assert_eq!(pair.second(), &5);
    }

    // Equality tests
    #[test]
    fn test_equality() {
        let pair1 = UnorderedPair::new(1, 2).unwrap();
        let pair2 = UnorderedPair::new(1, 2).unwrap();
        let pair3 = UnorderedPair::new(2, 1).unwrap(); // Different order
        let pair4 = UnorderedPair::new(1, 3).unwrap(); // Different second value

        assert_eq!(pair1, pair2);
        assert_eq!(pair1, pair3);
        assert_ne!(pair1, pair4);
    }

    #[test]
    fn test_same_elements() {
        let pair1 = UnorderedPair::new(1, 2).unwrap();
        let pair2 = UnorderedPair::new(2, 1).unwrap();
        let pair3 = UnorderedPair::new(1, 3).unwrap();

        assert_eq!(pair1, pair2); // Same elements, different order
        assert_ne!(pair1, pair3); // Different elements
    }

    #[test]
    fn test_equality_strings() {
        let pair1 = UnorderedPair::new("a", "b").unwrap();
        let pair2 = UnorderedPair::new("a", "b").unwrap();
        let pair3 = UnorderedPair::new("a", "c").unwrap();

        assert_eq!(pair1, pair2);
        assert_ne!(pair1, pair3);
    }

    // Edge cases
    #[test]
    fn test_large_values() {
        let pair = UnorderedPair::new(i32::MAX, i32::MIN).unwrap();
        assert!(pair.contains(&i32::MAX));
        assert!(pair.contains(&i32::MIN));
        assert!(!pair.contains(&0));
    }

    #[test]
    fn test_boolean_values() {
        let pair = UnorderedPair::new(true, false).unwrap();
        assert!(pair.contains(&true));
        assert!(pair.contains(&false));
    }

    #[test]
    #[should_panic]
    fn test_boolean_values_panic_on_same() {
        // Should panic because true != true is false
        let _pair = UnorderedPair::new(true, true).unwrap();
    }

    #[test]
    fn test_reference_types() {
        let x = 5;
        let y = 10;
        let pair = UnorderedPair::new(&x, &y).unwrap();

        assert!(pair.contains(&&5));
        assert!(pair.contains(&&10));
        assert!(!pair.contains(&&15));
    }

    // Additional method tests
    #[test]
    fn test_as_tuple() {
        let pair = UnorderedPair::new(100, 200).unwrap();
        assert_eq!(pair.as_tuple(), (&100, &200));
    }

    #[test]
    fn test_into_tuple() {
        let pair = UnorderedPair::new(100, 200).unwrap();
        let tuple = pair.into_tuple();
        assert_eq!(tuple, (100, 200));
    }

    #[test]
    fn test_contains_both() {
        let pair = UnorderedPair::new(1, 2).unwrap();
        assert!(pair.contains_both(&1, &2));
        assert!(pair.contains_both(&2, &1)); // Order doesn't matter
        assert!(!pair.contains_both(&1, &3));
        assert!(!pair.contains_both(&3, &4));
    }

    // Integration tests
    #[test]
    fn test_method_chaining() {
        let pair = UnorderedPair::new(100, 200).unwrap();

        // Test that all methods work together
        assert_eq!(pair.first(), &100);
        assert_eq!(pair.second(), &200);
        assert!(pair.contains(&100));
        assert!(pair.contains(&200));
        assert!(!pair.contains(&150));
        assert_eq!(pair.as_tuple(), (&100, &200));
    }

    #[test]
    fn test_with_complex_types() {
        #[derive(Debug, PartialEq, Eq)]
        struct Point {
            x: i32,
            y: i32,
        }

        let p1 = Point { x: 0, y: 0 };
        let p2 = Point { x: 1, y: 1 };
        let pair = UnorderedPair::new(p1, p2).unwrap();

        assert!(pair.contains(&Point { x: 0, y: 0 }));
        assert!(pair.contains(&Point { x: 1, y: 1 }));
        assert!(!pair.contains(&Point { x: 2, y: 2 }));
    }

    // Property-based test style
    #[test]
    fn test_contains_is_reflexive() {
        let pair = UnorderedPair::new(42, 99).unwrap();
        // If we check for a value that's in the pair, it should return true
        assert!(pair.contains(&42));
        assert!(pair.contains(&99));
    }

    #[test]
    fn test_new_or_panic_with_different_values() {
        let pair = UnorderedPair::new(7, 8).unwrap();
        assert_eq!(pair.first(), &7);
        assert_eq!(pair.second(), &8);
    }

    // Test the invariant in various scenarios
    #[test]
    fn test_invariant_preserved_by_all_constructors() {
        // new() preserves invariant
        assert!(UnorderedPair::new(1, 2).is_some());
        assert!(UnorderedPair::new(5, 5).is_none());

        // new().unwrap() preserves invariant
        let _pair = UnorderedPair::new(3, 4).unwrap();
        // Should panic if we try to create with equal values (tested above)

        // try_from() preserves invariant
        assert!(UnorderedPair::try_from((6, 7)).is_ok());
        assert!(UnorderedPair::try_from((8, 8)).is_err());
    }
}
