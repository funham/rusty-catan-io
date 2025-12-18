/// An unordered triple of three distinct values
#[derive(Debug, Eq)]
pub struct UnorderedTriple<T: PartialEq>(T, T, T);

impl<T: PartialEq> PartialEq for UnorderedTriple<T> {
    fn eq(&self, other: &Self) -> bool {
        // Check all permutations since order doesn't matter
        let (a1, b1, c1) = (&self.0, &self.1, &self.2);
        let (a2, b2, c2) = (&other.0, &other.1, &other.2);

        // Check all 6 possible permutations
        (a1 == a2 && b1 == b2 && c1 == c2)
            || (a1 == a2 && b1 == c2 && c1 == b2)
            || (a1 == b2 && b1 == a2 && c1 == c2)
            || (a1 == b2 && b1 == c2 && c1 == a2)
            || (a1 == c2 && b1 == a2 && c1 == b2)
            || (a1 == c2 && b1 == b2 && c1 == a2)
    }
}

impl<T: PartialEq> UnorderedTriple<T> {
    /// Creates a new UnorderedTriple from three values without checking for uniqueness.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it doesn't enforce the invariant that
    /// the three values must be different. Use `UnorderedTriple::new()` for safe construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// // Safe usage with distinct values
    /// let triple = unsafe { UnorderedTriple::new_unchecked(1, 2, 3) };
    /// assert_eq!(triple.first(), &1);
    /// assert_eq!(triple.second(), &2);
    /// assert_eq!(triple.third(), &3);
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// // Unsafe usage - violates invariant (but compiles)
    /// let triple = unsafe { UnorderedTriple::new_unchecked(5, 5, 6) };
    /// // This triple contains duplicate values, which violates the type's contract
    /// ```
    pub unsafe fn new_unchecked(a: T, b: T, c: T) -> Self {
        Self(a, b, c)
    }

    /// Creates a new UnorderedTriple from three distinct values.
    /// Returns `Some(UnorderedTriple)` if all values are different, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(1, 2, 3);
    /// assert!(triple.is_some());
    /// let triple = triple.unwrap();
    /// assert_eq!(triple.first(), &1);
    /// assert_eq!(triple.second(), &2);
    /// assert_eq!(triple.third(), &3);
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// // Same values are not allowed
    /// let triple = UnorderedTriple::new(5, 5, 6);
    /// assert!(triple.is_none());
    /// ```
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new("hello", "world", "rust");
    /// assert!(triple.is_some());
    /// let triple = triple.unwrap();
    /// assert!(triple.contains(&"hello"));
    /// assert!(triple.contains(&"world"));
    /// assert!(triple.contains(&"rust"));
    /// ```
    pub fn new(a: T, b: T, c: T) -> Option<Self> {
        if a != b && a != c && b != c {
            Some(Self(a, b, c))
        } else {
            None
        }
    }

    /// Checks if the unordered triple contains a given value
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(10, 20, 30).unwrap();
    /// assert!(triple.contains(&10));
    /// assert!(triple.contains(&20));
    /// assert!(triple.contains(&30));
    /// assert!(!triple.contains(&40));
    /// ```
    pub fn contains(&self, x: &T) -> bool {
        self.0 == *x || self.1 == *x || self.2 == *x
    }

    /// Returns the first element of the unordered triple
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(5, 10, 15).unwrap();
    /// assert_eq!(triple.first(), &5);
    /// ```
    pub fn first(&self) -> &T {
        &self.0
    }

    /// Returns the second element of the unordered triple
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(5, 10, 15).unwrap();
    /// assert_eq!(triple.second(), &10);
    /// ```
    pub fn second(&self) -> &T {
        &self.1
    }

    /// Returns the third element of the unordered triple
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(5, 10, 15).unwrap();
    /// assert_eq!(triple.third(), &15);
    /// ```
    pub fn third(&self) -> &T {
        &self.2
    }

    /// Returns all three elements as a tuple
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(7, 8, 9).unwrap();
    /// assert_eq!(triple.as_tuple(), (&7, &8, &9));
    /// ```
    pub fn as_tuple(&self) -> (&T, &T, &T) {
        (&self.0, &self.1, &self.2)
    }

    /// Returns all three elements as a tuple of owned values
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(7, 8, 9).unwrap();
    /// assert_eq!(triple.into_tuple(), (7, 8, 9));
    /// ```
    pub fn into_tuple(self) -> (T, T, T) {
        (self.0, self.1, self.2)
    }

    /// Returns `true` if the unordered triple contains all given values
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(1, 2, 3).unwrap();
    /// assert!(triple.contains_all(&[&1, &2, &3]));
    /// assert!(triple.contains_all(&[&3, &1, &2])); // Order doesn't matter
    /// assert!(!triple.contains_all(&[&1, &2, &4]));
    /// ```
    pub fn contains_all(&self, values: &[&T]) -> bool {
        values.iter().all(|&x| self.contains(x))
    }

    /// Returns `true` if the unordered triple contains both given values
    /// (Convenience method similar to UnorderedPair::contains_both)
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple = UnorderedTriple::new(1, 2, 3).unwrap();
    /// assert!(triple.contains_both(&1, &2));
    /// assert!(triple.contains_both(&3, &1)); // Order doesn't matter
    /// assert!(!triple.contains_both(&1, &4));
    /// ```
    pub fn contains_both(&self, a: &T, b: &T) -> bool {
        self.contains(a) && self.contains(b)
    }

    /// Checks if this unordered triple has the same elements as another (order-independent)
    ///
    /// # Examples
    ///
    /// ```
    /// use rusty_catan_io::common::unordered_triple::UnorderedTriple;
    ///
    /// let triple1 = UnorderedTriple::new(1, 2, 3).unwrap();
    /// let triple2 = UnorderedTriple::new(3, 1, 2).unwrap();
    /// let triple3 = UnorderedTriple::new(1, 2, 4).unwrap();
    /// assert!(triple1.same_elements(&triple2));
    /// assert!(!triple1.same_elements(&triple3));
    /// ```
    pub fn same_elements(&self, other: &Self) -> bool {
        self == other // Uses our custom PartialEq implementation
    }
}

impl<T: PartialEq> TryFrom<(T, T, T)> for UnorderedTriple<T> {
    type Error = &'static str;

    fn try_from(value: (T, T, T)) -> Result<Self, Self::Error> {
        let (a, b, c) = value;
        if a != b && a != c && b != c {
            Ok(Self(a, b, c))
        } else {
            Err("UnorderedTriple requires three distinct values")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_distinct_values() {
        let triple = UnorderedTriple::new(1, 2, 3);
        assert!(triple.is_some());
        let triple = triple.unwrap();
        assert_eq!(triple.first(), &1);
        assert_eq!(triple.second(), &2);
        assert_eq!(triple.third(), &3);
    }

    #[test]
    fn test_new_with_equal_values() {
        // Test all combinations of equal values
        assert!(UnorderedTriple::new(5, 5, 6).is_none()); // First two equal
        assert!(UnorderedTriple::new(5, 6, 5).is_none()); // First and third equal
        assert!(UnorderedTriple::new(6, 5, 5).is_none()); // Last two equal
        assert!(UnorderedTriple::new(5, 5, 5).is_none()); // All three equal
    }

    #[test]
    fn test_from_tuple() {
        let triple: UnorderedTriple<i32> = unsafe { UnorderedTriple::new_unchecked(3, 4, 5) };
        assert_eq!(triple, UnorderedTriple(3, 4, 5));
    }

    // Contains method tests
    #[test]
    fn test_contains_integers() {
        let triple = UnorderedTriple::new(10, 20, 30).unwrap();

        // Positive cases
        assert!(triple.contains(&10));
        assert!(triple.contains(&20));
        assert!(triple.contains(&30));

        // Negative cases
        assert!(!triple.contains(&5));
        assert!(!triple.contains(&15));
        assert!(!triple.contains(&25));
        assert!(!triple.contains(&35));
    }

    #[test]
    fn test_contains_strings() {
        let triple = UnorderedTriple::new(
            "apple".to_string(),
            "banana".to_string(),
            "cherry".to_string(),
        )
        .unwrap();

        assert!(triple.contains(&"apple".to_string()));
        assert!(triple.contains(&"banana".to_string()));
        assert!(triple.contains(&"cherry".to_string()));
        assert!(!triple.contains(&"date".to_string()));
    }

    // Test that unsafe constructor allows duplicates
    #[test]
    fn test_unsafe_constructor_allows_duplicates() {
        let triple = unsafe { UnorderedTriple::new_unchecked(5, 5, 6) };
        // This is allowed by the unsafe constructor but violates the type's invariant
        assert!(triple.contains(&5));
        assert!(triple.contains(&6));
        assert_eq!(triple.first(), &5);
        assert_eq!(triple.second(), &5);
        assert_eq!(triple.third(), &6);
    }

    // Equality tests (order-independent)
    #[test]
    fn test_equality_different_orders() {
        let triple1 = UnorderedTriple::new(1, 2, 3).unwrap();
        let triple2 = UnorderedTriple::new(1, 2, 3).unwrap();
        let triple3 = UnorderedTriple::new(3, 1, 2).unwrap();
        let triple4 = UnorderedTriple::new(2, 3, 1).unwrap();
        let triple5 = UnorderedTriple::new(1, 3, 2).unwrap();
        let triple6 = UnorderedTriple::new(2, 1, 3).unwrap();
        let triple7 = UnorderedTriple::new(3, 2, 1).unwrap();

        // All permutations should be equal
        assert_eq!(triple1, triple2);
        assert_eq!(triple1, triple3);
        assert_eq!(triple1, triple4);
        assert_eq!(triple1, triple5);
        assert_eq!(triple1, triple6);
        assert_eq!(triple1, triple7);
    }

    #[test]
    fn test_inequality_different_values() {
        let triple1 = UnorderedTriple::new(1, 2, 3).unwrap();
        let triple2 = UnorderedTriple::new(1, 2, 4).unwrap();
        let triple3 = UnorderedTriple::new(4, 5, 6).unwrap();

        assert_ne!(triple1, triple2);
        assert_ne!(triple1, triple3);
    }

    #[test]
    fn test_same_elements() {
        let triple1 = UnorderedTriple::new(1, 2, 3).unwrap();
        let triple2 = UnorderedTriple::new(3, 1, 2).unwrap();
        let triple3 = UnorderedTriple::new(1, 2, 4).unwrap();

        assert!(triple1.same_elements(&triple2));
        assert!(!triple1.same_elements(&triple3));
    }

    // Edge cases
    #[test]
    fn test_large_values() {
        let triple = UnorderedTriple::new(i32::MAX, i32::MIN, 0).unwrap();
        assert!(triple.contains(&i32::MAX));
        assert!(triple.contains(&i32::MIN));
        assert!(triple.contains(&0));
        assert!(!triple.contains(&1));
    }

    #[test]
    fn test_boolean_values() {
        // Need three distinct booleans... oh wait, there are only two booleans!
        // This demonstrates that UnorderedTriple<bool> can never be created
        assert!(UnorderedTriple::new(true, false, true).is_none());
        assert!(UnorderedTriple::new(true, true, false).is_none());
        assert!(UnorderedTriple::new(false, false, true).is_none());
    }

    #[test]
    fn test_reference_types() {
        let x = 5;
        let y = 10;
        let z = 15;
        let triple = UnorderedTriple::new(&x, &y, &z).unwrap();

        assert!(triple.contains(&&5));
        assert!(triple.contains(&&10));
        assert!(triple.contains(&&15));
        assert!(!triple.contains(&&20));
    }

    // Additional method tests
    #[test]
    fn test_as_tuple() {
        let triple = UnorderedTriple::new(100, 200, 300).unwrap();
        assert_eq!(triple.as_tuple(), (&100, &200, &300));
    }

    #[test]
    fn test_into_tuple() {
        let triple = UnorderedTriple::new(100, 200, 300).unwrap();
        let tuple = triple.into_tuple();
        assert_eq!(tuple, (100, 200, 300));
    }

    #[test]
    fn test_contains_all() {
        let triple = UnorderedTriple::new(1, 2, 3).unwrap();

        // Positive cases
        assert!(triple.contains_all(&[&1, &2, &3]));
        assert!(triple.contains_all(&[&3, &1, &2]));
        assert!(triple.contains_all(&[&2, &3, &1]));
        assert!(triple.contains_all(&[&1, &1, &2])); // Duplicates in input

        // Negative cases
        assert!(!triple.contains_all(&[&1, &2, &4]));
        assert!(!triple.contains_all(&[&4, &5, &6]));
    }

    #[test]
    fn test_contains_both() {
        let triple = UnorderedTriple::new(1, 2, 3).unwrap();
        assert!(triple.contains_both(&1, &2));
        assert!(triple.contains_both(&3, &1));
        assert!(triple.contains_both(&2, &3));
        assert!(!triple.contains_both(&1, &4));
        assert!(!triple.contains_both(&4, &5));
    }

    // Integration tests
    #[test]
    fn test_method_chaining() {
        let triple = UnorderedTriple::new(100, 200, 300).unwrap();

        // Test that all methods work together
        assert_eq!(triple.first(), &100);
        assert_eq!(triple.second(), &200);
        assert_eq!(triple.third(), &300);
        assert!(triple.contains(&100));
        assert!(triple.contains(&200));
        assert!(triple.contains(&300));
        assert!(!triple.contains(&150));
        assert_eq!(triple.as_tuple(), (&100, &200, &300));
        assert!(triple.contains_both(&100, &200));
        assert!(triple.contains_all(&[&100, &200, &300]));
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
        let p3 = Point { x: 2, y: 2 };
        let triple = UnorderedTriple::new(p1, p2, p3).unwrap();

        assert!(triple.contains(&Point { x: 0, y: 0 }));
        assert!(triple.contains(&Point { x: 1, y: 1 }));
        assert!(triple.contains(&Point { x: 2, y: 2 }));
        assert!(!triple.contains(&Point { x: 3, y: 3 }));
    }

    #[test]
    fn test_try_from_tuple_success() {
        let triple: Result<UnorderedTriple<i32>, _> = UnorderedTriple::try_from((3, 4, 5));
        assert!(triple.is_ok());
        let triple = triple.unwrap();
        assert_eq!(triple.first(), &3);
        assert_eq!(triple.second(), &4);
        assert_eq!(triple.third(), &5);
    }

    #[test]
    fn test_try_from_tuple_failure() {
        // Test all failing cases
        assert!(UnorderedTriple::try_from((5, 5, 6)).is_err());
        assert!(UnorderedTriple::try_from((5, 6, 5)).is_err());
        assert!(UnorderedTriple::try_from((6, 5, 5)).is_err());
        assert!(UnorderedTriple::try_from((5, 5, 5)).is_err());

        let result: Result<UnorderedTriple<i32>, _> = UnorderedTriple::try_from((5, 5, 6));
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            "UnorderedTriple requires three distinct values"
        );
    }

    // Test the invariant in various scenarios
    #[test]
    fn test_invariant_preserved_by_all_constructors() {
        // new() preserves invariant
        assert!(UnorderedTriple::new(1, 2, 3).is_some());
        assert!(UnorderedTriple::new(5, 5, 6).is_none());

        // new().unwrap() preserves invariant
        let _triple = UnorderedTriple::new(3, 4, 5).unwrap();

        // try_from() preserves invariant
        assert!(UnorderedTriple::try_from((6, 7, 8)).is_ok());
        assert!(UnorderedTriple::try_from((8, 8, 9)).is_err());
    }

    // Performance/edge case: very large number of permutations
    #[test]
    fn test_equality_with_identical_but_different_instances() {
        let triple1 = unsafe { UnorderedTriple::new_unchecked(1, 2, 3) };
        let triple2 = unsafe { UnorderedTriple::new_unchecked(1, 2, 3) };
        assert_eq!(triple1, triple2);
    }

    #[test]
    fn test_hash_consistency() {
        // Note: We don't derive Hash, but if we did, we'd want to test this
        let triple1 = UnorderedTriple::new(1, 2, 3).unwrap();
        let triple2 = UnorderedTriple::new(3, 1, 2).unwrap();
        let triple3 = UnorderedTriple::new(1, 2, 4).unwrap();

        // These should be equal according to PartialEq
        assert_eq!(triple1, triple2);
        assert_ne!(triple1, triple3);
    }
}
