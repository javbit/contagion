extern crate tokio;
#[macro_use]
extern crate futures;
extern crate bytes;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate docopt;
extern crate ropey;
extern crate smallvec;
extern crate termion;
extern crate unicode_segmentation;
extern crate unicode_width;


mod network;
mod message;
mod text_ui;

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

    // Get filename for writing
    let mut filename = String::new();
    println!("Enter filename for writing messages");
    let _ = std::io::stdin().read_line(&mut filename);
    filename.pop();

    // Get filename for storing all messages
    let mut all_messages = String::new();
    println!("Enter filename for storing all messages");
    let _ = std::io::stdin().read_line(&mut all_messages);
    all_messages.pop();

    // Create channels.
    let (ui_msg_send, user_input) = mpsc::unbounded();
    let (user_output, ui_msg_recv) = mpsc::unbounded();

    // Spawn thread for networking.
    let node_network = thread::spawn(|| network::reactor(
        addr,
        port,
        bootstrap,
        user_input,
        user_output
    ));

    println!("Multi-threaded!");

    text_ui::start_ui(ui_msg_send, ui_msg_recv, filename, all_messages);

    node_network.join().expect("Spawn has panicked!");
}
