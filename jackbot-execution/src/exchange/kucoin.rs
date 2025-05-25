//! Jackpot order execution for Kucoin.
//!
//! Kucoin exposes leverage settings but no mechanism for enforcing a fixed
//! ticket loss. Until such support exists this function always returns an
//! error.
#![allow(dead_code)]

pub fn place_jackpot_order() -> Result<(), &'static str> {
    Err("jackpot orders not yet implemented for Kucoin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub() {
        assert!(place_jackpot_order().is_err());
    }
}
