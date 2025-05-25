//! Jackpot order execution for Bybit.
//!
//! Bybit exposes leverage configuration but not a direct API for ticket based
//! liquidation. Jackpot orders are therefore not yet implemented and this
//! function simply returns an error.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Bybit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
