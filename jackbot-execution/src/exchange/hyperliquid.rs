//! Jackpot order execution for Hyperliquid.
//!
//! Hyperliquid only offers perpetual futures and does not provide a ticket
//! based liquidation mechanism via the API. Jackpot orders are unimplemented
//! and this function returns an error.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Hyperliquid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
