#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#![deny(clippy::all)]

pub mod compress;
pub mod filters;
pub mod mode;
pub mod sections;
pub mod utf16;

#[cfg(target_arch = "wasm32")]
mod abi;
