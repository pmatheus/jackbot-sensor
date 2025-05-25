//! Jackpot order execution for Crypto.com.
//!
//! Crypto.com's API does not document isolated margin endpoints with a ticket
//! loss parameter. The module therefore returns an error until proper support
//! is confirmed.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Crypto.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
