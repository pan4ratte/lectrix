//! All PDF logic for Lectrix. No Tauri dependency.

pub mod annot;
pub mod compress;
pub mod docinfo;
pub mod error;
pub mod ffi;
pub mod fonts;
pub mod geometry;
pub mod labels;
pub mod links;
pub mod merge;
pub mod objects;
pub mod ops;
pub mod outline;
pub mod pages;
pub mod platform;
pub mod properties;
pub mod protection;
pub mod render;
pub mod save;
pub mod session;
pub mod testgen;
pub mod text;
pub mod unprotect;

pub use error::{Error, Result};

// libjpeg, which Compress's encoder calls from the C shim (ADR 0027).
use mozjpeg_sys as _;
