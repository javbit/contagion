extern crate tokio;
#[macro_use]
extern crate futures;
extern crate bytes;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod network;
mod message;

use std::thread;

use futures::sync::mpsc;

fn main() {
    // Get IP address to use (default to 127.0.0.1).
    let mut addr = String::new();
    println!("Enter address to use (default: localhost)");
    let _ = std::io::stdin().read_line(&mut addr);

    // Get port number (default to 6142)
    let mut port = String::new();
    println!("Enter port to use (default: 6142)");
    let _ = std::io::stdin().read_line(&mut port);

    // Get bootstrap node (optional)
    let mut bootstrap = String::new();
    println!("Enter bootstrap node address (optional)");
    let _ = std::io::stdin().read_line(&mut bootstrap);

    // Create channels.
    let (ui_msg_send, user_input) = mpsc::unbounded();
    let (user_output, ui_msg_recv) = mpsc::unbounded();

    let node = thread::spawn(|| network::reactor(
        addr,
        port,
        bootstrap,
        user_input,
        user_output
    ));
    println!("Multi-threaded!");
    // TODO: Start UI process here. Can remove `join` then.
    node.join().expect("Spawn has panicked!");
}
