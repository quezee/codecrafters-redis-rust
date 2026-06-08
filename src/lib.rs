pub mod resp_value;
pub mod resp_parser;
pub mod resp_handler;

use std::{collections::HashMap, sync::{Arc, Mutex}};


pub type Bytes = Vec<u8>;
pub type Storage = Arc<Mutex<HashMap<Bytes, resp_value::Value>>>;
