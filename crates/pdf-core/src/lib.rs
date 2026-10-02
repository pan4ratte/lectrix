//! All PDF logic for Folio. No Tauri dependency.

pub mod annot;
pub mod error;
pub mod ffi;
pub mod geometry;
pub mod labels;
pub mod merge;
pub mod objects;
pub mod outline;
pub mod render;
pub mod save;
pub mod session;
pub mod testgen;

pub use error::{Error, Result};
