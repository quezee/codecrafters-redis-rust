#![allow(unused_imports)]use std::{collections::HashMap, error::Error, io::{self, Read, Write}, thread};
use std::sync::{Arc, Mutex};

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{BufReader};

use codecrafters_redis::{Bytes, Storage};
use codecrafters_redis::resp_value::Value;
use codecrafters_redis::resp_parser::{AnyParser, Parser, RespError};
use codecrafters_redis::resp_handler::handle_request;


#[tokio::main]
async fn main() -> Result<(), RespError> {
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    let storage = Storage::default();
    
    loop {
        let (stream, _) = listener.accept().await?;
        let (stream_reader, mut stream_writer) = stream.into_split();
        let storage_ptr = storage.clone();
        let fut = async move {
            println!("accepted new connection");
            let mut stream_reader = BufReader::new(stream_reader);
            loop {
                let result: Result<(), RespError> = async {
                    let request = AnyParser::deserialize(&mut stream_reader).await?;
                    let response = handle_request(request, &storage_ptr)?;
                    response.serialize_to_stream(&mut stream_writer).await?;
                    Ok(())
                }.await;
                if let Err(e) = result {
                    eprint!("{e:?}");
                    break;
                }
            }
        };
        tokio::spawn(fut);
    }
}
