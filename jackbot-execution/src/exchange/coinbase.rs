//! Jackpot order execution for Coinbase.
//!
//! This is currently a stub. API integration will be added in the future.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Coinbase")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
