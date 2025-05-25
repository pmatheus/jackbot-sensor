//! Jackpot order execution for Gate.io.
//!
//! Gate.io's API lacks documented support for isolated leverage orders with a
//! predetermined loss limit. This module is a stub that returns an error until
//! proper endpoints are confirmed.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Gate.io")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
