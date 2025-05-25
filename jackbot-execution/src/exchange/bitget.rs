//! Jackpot order execution for Bitget.
//!
//! Bitget's API does not yet expose isolated high leverage orders with a fixed
//! ticket loss. Until these endpoints are available this function returns an
//! error.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Bitget")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
