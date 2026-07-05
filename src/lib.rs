pub mod resp_value;
pub mod resp_parser;
pub mod resp_handler;

use std::{collections::{HashMap, LinkedList}, time::Instant, sync::{Arc, Mutex}};


pub type Bytes = Vec<u8>;
pub type Storage = Arc<Mutex<HashMap<Bytes, StorageEntry>>>;

#[derive(Debug, PartialEq, Clone)]
enum StoredValue {
    Value(resp_value::Value),
    List(LinkedList<Bytes>),
}

#[derive(Debug, PartialEq)]
pub struct StorageEntry {
    value: StoredValue,
    expires_at: Option<Instant>,
}

impl StorageEntry {
    fn new(value: resp_value::Value, expires_at: Option<Instant>) -> Self {
        Self {value: value.into(), expires_at}
    }
}

impl From<resp_value::Value> for StoredValue {
    fn from(value: resp_value::Value) -> Self {
        StoredValue::Value(value)
    }
}

impl From<LinkedList<Bytes>> for StoredValue {
    fn from(value: LinkedList<Bytes>) -> Self {
        Self::List(value)
    }
}

impl From<resp_value::Value> for StorageEntry {
    fn from(value: resp_value::Value) -> Self {
        Self {value: value.into(), expires_at: None}
    }
}

impl From<LinkedList<Bytes>> for StorageEntry {
    fn from(value: LinkedList<Bytes>) -> Self {
        Self {value: value.into(), expires_at: None}
    }
}
