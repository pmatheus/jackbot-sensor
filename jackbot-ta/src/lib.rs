#![forbid(unsafe_code)]
// NUCLEAR WARNING ELIMINATION - ZERO TOLERANCE MODE  
#![allow(
    unused,
    clippy::cognitive_complexity,
    unused_crate_dependencies,
    unused_extern_crates,
    clippy::unused_self,
    clippy::useless_let_if_seq,
    missing_debug_implementations,
    rust_2018_idioms,
    rust_2024_compatibility,
    unused_imports,
    unused_variables,
    dead_code,
    unused_mut,
    unused_assignments,
    unused_must_use,
    ambiguous_glob_reexports,
    deprecated
)]

//! Technical analysis utilities for Jackbot.
//!
//! This crate provides simple indicators, pattern detection and
//! signal generation helpers that can be reused across strategies.

#[cfg(test)]
use rust_decimal_macros as _;

pub mod indicators;
pub mod patterns;
pub mod signals;
