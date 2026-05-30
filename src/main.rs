#![allow(unused_imports)]
use std::{error::Error, io::{Read, Write}, net::TcpListener};

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                let mut buf = vec![];
                let bytes_read = stream.read(&mut buf)?;
                println!("read bytes: {bytes_read}");
                stream.write_all(b"+PONG\r\n")?;
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
    Ok(())
}
