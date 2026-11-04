//! Core domain logic for nahshot: file naming, storage and gallery metadata.

pub mod db;
pub mod error;
pub mod library;
pub mod model;
pub mod naming;

pub use error::{CoreError, Result};
