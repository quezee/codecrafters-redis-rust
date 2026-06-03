#![allow(unused_imports)]
use std::{io, io::{Read, Write}, error::Error, net::TcpListener, thread};

use codecrafters_redis::resp_value::Value;
use codecrafters_redis::resp_parser::{AnyParser, Parser, RespError};

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                let _handle = thread::spawn(move || -> Result<(), RespError> {
                    loop {
                        let mut stream_reader = io::BufReader::new(stream.try_clone()?);
                        let value = AnyParser::deserialize(&mut stream_reader)?;

                        // let mut stdout = io::BufWriter::new(io::stdout().lock());
                        // value.serialize(&mut stdout)?;

                        if let Value::Arr(Some(arr)) = value {
                            if arr.is_empty() {
                                Value::Err("empty message".into()).serialize(&mut stream)?;
                                continue;
                            }
                            if let Value::BulkStr(Some(cmd)) = &arr[0] {
                                match cmd.to_lowercase().as_str() {
                                    "ping" => {
                                        stream.write_all(b"+PONG\r\n")?;
                                    },
                                    "echo" => {
                                        if arr.len() == 1 {
                                            Value::Err("ECHO with no string received".into()).serialize(&mut stream)?;
                                        } else {
                                            if let Value::BulkStr(_) = arr[1] {
                                                arr[1].serialize(&mut stream)?;
                                            } else {
                                                Value::Err("ECHO should contain a bulk string".into()).serialize(&mut stream)?;
                                            }
                                        }
                                    },
                                    _ => {
                                        Value::Err("Unknown command: {cmd}".into()).serialize(&mut stream)?;
                                    }
                                }
                            }
                        }
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
