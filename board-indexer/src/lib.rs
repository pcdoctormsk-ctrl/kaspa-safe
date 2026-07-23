//! Infrastructure-neutral KBRD indexer core.
//!
//! The crate deliberately contains only data derivable from Kaspa transaction payloads:
//! envelope parsing and verification, deterministic projection into SQLite, scan progress,
//! and a read-only HTTP projection. Image bytes, moderation, reports, notifications, and
//! operator controls belong to deployments, not to this package.

pub mod api;
pub mod envelope;
pub mod indexer;
#[cfg(feature = "node")]
pub mod node;
pub mod store;

pub use envelope::{parse_and_verify, BoardParseError, BoardPost};
pub use indexer::{flatten_sorted, process_txs, ProcessConfig, TxItem};
pub use store::{CatalogRow, IngestOutcome, PostRow, StatusRow, Store, ThreadView};
