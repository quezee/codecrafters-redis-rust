#![allow(unused_imports)]
use std::{error::Error, io::{Read, Write}, net::TcpListener, thread};

fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                println!("accepted new connection");
                let _handle = thread::spawn(move || {
                    let mut buf = [0; 512];
                    loop {
                        let bytes_read = stream.read(&mut buf).unwrap();
                        println!("read bytes: {bytes_read}");
                        if bytes_read == 0 {
                            break;
                        }
                        stream.write_all(b"+PONG\r\n").unwrap();
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
