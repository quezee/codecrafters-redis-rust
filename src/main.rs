#![allow(unused_imports)]
use std::{error::Error, io::{Read, Write}, net::TcpListener};

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                loop {
                    let mut buf: Vec<u8> = vec![];
                    let bytes_read = stream.read_to_end(&mut buf)?;
                    println!("read bytes: {bytes_read}");
                    if bytes_read == 0 {
                        break;
                    }
                    stream.write_all(b"+PONG\r\n")?;
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
    Ok(())
}
