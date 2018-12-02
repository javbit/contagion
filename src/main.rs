extern crate tokio;
#[macro_use]
extern crate futures;
extern crate bytes;

mod network;

fn main() {
    network::reactor();
}
