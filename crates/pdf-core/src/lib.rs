//! All PDF logic for Folio. No Tauri dependency.

pub mod annot;
pub mod docinfo;
pub mod error;
pub mod ffi;
pub mod fonts;
pub mod geometry;
pub mod labels;
pub mod merge;
pub mod objects;
pub mod ops;
pub mod outline;
pub mod platform;
pub mod render;
pub mod save;
pub mod session;
pub mod testgen;
pub mod text;

pub use error::{Error, Result};
