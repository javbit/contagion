extern crate tokio;

use tokio::io;
use tokio::net::TcpStream;
use tokio::prelude::*;

fn main() {
    let addr = "127.0.0.1:1234".parse().unwrap();
    let client = TcpStream::connect(&addr)
        .and_then(|stream| {
            println!("Created stream");
            io::write_all(stream, "Hello, world!\n").then(|result| {
                println!("Wrote to stream: {}", result.is_ok());
                Ok(())
            })
        }).map_err(|err| {
            println!("Connection error: {:?}", err);
        });
    println!("About to create the stream and write to it...");
    tokio::run(client);
    println!("Stream has been created and written to");
}
