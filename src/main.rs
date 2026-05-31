#![allow(unused_imports)]
use std::{io, io::{Read, Write}, error::Error, net::TcpListener, thread};

use codecrafters_redis::resp_value::Value;
use codecrafters_redis::resp_parser::{AnyParser, Parser};

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                // let _handle = thread::spawn(move || -> io::Result<()> {
                //     let stream_reader = io::BufReader::new(stream.try_clone()?);
                //     let value = AnyParser::deserialize(stream_reader)?;
                //     if let Value::Arr(values) = value {
                //         if let Value::BulkStr(s) = values[0] {
                //             if s.to_lowercase() == "echo" {
                //                 // let value_ser: &[u8] = values[1].serialize();
                //                 // stream.write_all(value_ser)?;
                //             }
                //         }
                //     }
                //     Ok(())
                // });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
    Ok(())
}
