//! Saga execution context.
//!
//! [`CommitCtx`] carries the [`Storage`] backend through all saga steps.
//! It implements `Serialize`/`Deserialize` as no-ops because our sagas
//! always run to completion — we never pause, persist, or resume.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::storage::Storage;

/// Shared context threaded through every saga step.
///
/// Holds a reference-counted handle to the storage backend so each
/// step can read and write without owning the storage.
pub struct CommitCtx {
    pub(crate) storage: Arc<dyn Storage>,
}

// legend requires Serialize/Deserialize bounds on the Execution<Ctx, …> type.
// We satisfy them with stubs because the saga always runs to completion in a
// single await — serialization is never actually invoked.
impl Serialize for CommitCtx {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for CommitCtx {
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("CommitCtx cannot be deserialized"))
    }
}

// SAFETY: Arc<dyn Storage> is Send + Sync because Storage: Send + Sync.
unsafe impl Send for CommitCtx {}
unsafe impl Sync for CommitCtx {}
