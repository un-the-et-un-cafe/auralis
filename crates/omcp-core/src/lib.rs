//! OMCP core stub for M0 CI. Spec and real types land with M0 protocol push.

/// Trivial identity helper so CI has a public API and unit test to exercise.
pub fn ping() -> &'static str {
    "omcp"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_omcp() {
        assert_eq!(ping(), "omcp");
    }
}
