pub mod resp_value;
pub mod resp_parser;
pub mod resp_handler;

use std::{collections::HashMap, time::Instant, sync::{Arc, Mutex}};


pub type Bytes = Vec<u8>;
pub type Storage = Arc<Mutex<HashMap<Bytes, Entry>>>;

#[derive(Debug, PartialEq)]
pub struct Entry {
    value: resp_value::Value,
    expires_at: Option<Instant>,
}

impl Entry {
    fn new(value: resp_value::Value, expires_at: Option<Instant>) -> Self {
        Self {value, expires_at}
    }
}

impl From<resp_value::Value> for Entry {
    fn from(value: resp_value::Value) -> Self {
        Self {value, expires_at: None}
    }
}
