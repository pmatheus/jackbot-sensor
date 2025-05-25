//! Jackpot order execution for Coinbase.
//!
//! Coinbase does not offer futures trading or high leverage positions via the
//! public API, so jackpot orders are unsupported and this function simply
//! returns an error.
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
