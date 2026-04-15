use crate::storage_tests;

use super::MemoryStorage;

storage_tests!(async { MemoryStorage::new() });
