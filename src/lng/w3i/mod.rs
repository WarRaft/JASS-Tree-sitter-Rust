//! Binary reader for Warcraft III map information files (`.w3i`).
//!
//! The format is described in `w3i.hexpat` (ImHex pattern) and at
//! <https://xgm.guru/p/wc3/w3-file-format>.
//!
//! All multi-byte integers are **little-endian**.

pub mod parse;
pub mod send;

pub use parse::*;
