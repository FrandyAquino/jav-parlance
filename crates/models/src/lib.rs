//! Model management: a pinned catalog, a local store and verified downloads.
//!
//! Every artifact is pinned by SHA-256. A file is only moved into place after
//! its digest matches, so a truncated or tampered download can never be
//! loaded by an inference engine.

mod catalog;
mod download;
mod store;

pub use catalog::{Archive, CatalogEntry, catalog, find};
pub use download::{DownloadError, download};
pub use store::{ModelStore, StoreError};
