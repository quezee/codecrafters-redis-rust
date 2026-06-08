#![allow(unused_imports)]
use std::{collections::HashMap, error::Error, io::{self, Read, Write}, net::TcpListener, thread};
use std::sync::{Arc, Mutex};

use codecrafters_redis::{Bytes, Storage};
use codecrafters_redis::resp_value::Value;
use codecrafters_redis::resp_parser::{AnyParser, Parser, RespError};
use codecrafters_redis::resp_handler::handle_request;


fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    let storage = Storage::default();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                let storage_ptr = storage.clone();
                let _handle = thread::spawn(move || -> Result<(), RespError> {
                    loop {
                        let mut stream_reader = io::BufReader::new(stream.try_clone()?);
                        let request = AnyParser::deserialize(&mut stream_reader)?;
                        let response = handle_request(request, &storage_ptr)?;
                        response.serialize(&mut stream)?;
                    }
                });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
    Ok(())
}
