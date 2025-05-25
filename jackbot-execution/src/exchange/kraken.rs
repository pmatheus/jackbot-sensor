//! Jackpot order execution for Kraken.
//!
//! Kraken's API does not expose isolated high leverage endpoints or ticket
//! based liquidation. Jackpot orders are therefore unimplemented and this
//! function returns an error.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Kraken")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
