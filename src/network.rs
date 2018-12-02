use bytes::{BufMut, Bytes, BytesMut};
use futures::sync::mpsc;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

/// Shorthand for the transmit half of the message channel.
type Tx = mpsc::UnboundedSender<Bytes>;

/// Shorthand for the receive half of the message channel.
type Rx = mpsc::UnboundedReceiver<Bytes>;

/// Data that is shared between all peers in the chat server.
///
/// This is the set of `Tx` handles for all connected clients. Whenever a
/// message is received from a client, it is broadcasted to all peers by
/// iterating over the `peers` entries and sending a copy of the message on each
/// `Tx`.
struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}

/// The state for each connected client.
struct Peer {
    /// The TCP socket wrapped with the `Messages` codec, defined below.
    ///
    /// This handles sending and receiving data on the socket. When using
    /// `Messages`, we can work at the message level instead of having to manage
    /// the raw byte operations.
    messages: Messages,

    /// Handle to the shared chat state.
    ///
    /// This is used to broadcast messages read off the socket to all connected
    /// peers.
    state: Arc<Mutex<Shared>>,

    /// Receive half of the message channel.
    ///
    /// This is used to receive messages from peers. When a message is received
    /// off of this `Rx`, it will be written to the socket.
    rx: Rx,

    /// Client socket address.
    ///
    /// The socket address is used as the key in the `peers` HashMap. The
    /// address is saved so that the `Peer` drop implementation can clean up its
    /// entry.
    addr: SocketAddr,
}

/// Message based codec
///
/// This decorates a socket and presents a message based read / write interface.
///
/// As a user of `Messages`, we can focus on working at the message level. So,
/// we send and receive values that represent entire messages. The `Messages`
/// codec will handle the encoding and decoding as well as reading from and
/// writing to the socket.
#[derive(Debug)]
struct Messages {
    /// The TCP socket.
    socket: TcpStream,

    /// Buffer used when reading from the socket. Data is not returned from this
    /// buffer until an entire message has been read.
    rd: BytesMut,

    /// Buffer used to stage data before writing it to the socket.
    wr: BytesMut,
}

impl Shared {
    /// Create a new, empty, instance of `Shared`.
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }
}

impl Peer {
    /// Create a new instance of `Peer`.
    fn new(state: Arc<Mutex<Shared>>, messages: Messages) -> Peer {
        // Get the client socket address
        let addr = messages.socket.peer_addr().unwrap();

        // Create a channel for this peer
        let (tx, rx) = mpsc::unbounded();

        // Add an entry for this `Peer` in the shared state map.
        state.lock().unwrap().peers.insert(addr, tx);

        Peer {
            // name,
            messages,
            state,
            rx,
            addr,
        }
    }
}

/// This is where a connected client is managed.
///
/// A `Peer` is also a future representing completely processing the client.
///
/// When the socket closes, the `Peer` future completes.
///
/// While processing, the peer future implementation will:
///
/// 1) Receive messages on its message channel and write them to the socket.
/// 2) Receive messages from the socket and broadcast them to all peers.
///
impl Future for Peer {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        // Tokio (and futures) use cooperative scheduling without any
        // preemption. If a task never yields execution back to the executor,
        // then other tasks may be starved.
        //
        // To deal with this, robust applications should not have any unbounded
        // loops. In this example, we will read at most `MESSAGES_PER_TICK`
        // messages from the client on each tick.
        //
        // If the limit is hit, the current task is notified, informing the
        // executor to schedule the task again asap.
        const MESSAGES_PER_TICK: usize = 10;

        // Receive all messages from peers.
        for i in 0..MESSAGES_PER_TICK {
            // Polling an `UnboundedReceiver` cannot fail, so `unwrap` here is
            // safe.
            match self.rx.poll().unwrap() {
                Async::Ready(Some(v)) => {
                    // Buffer the message. Once all messages are buffered, they
                    // will be flushed to the socket (right below).
                    self.messages.buffer(&v);

                    // If this is the last iteration, the loop will break even
                    // though there could still be messages to read. Because we
                    // did not reach `Async::NotReady`, we have to notify
                    // ourselves in order to tell the executor to schedule the
                    // task again.
                    if i + 1 == MESSAGES_PER_TICK {
                        task::current().notify();
                    }
                }
                _ => break,
            }
        }

        // Flush the write buffer to the socket
        let _ = self.messages.poll_flush()?;

        // Read new messages from the socket
        while let Async::Ready(message) = self.messages.poll()? {
            println!("Received message: {:?}", message);

            if let Some(message) = message {
                let mut message = message.clone();
                message.extend_from_slice(b"\r\n");

                // We're using `Bytes`, which allows zero-copy clones (by
                // storing the data in an Arc internally).
                //
                // However, before cloning, we must freeze the data. This
                // converts it from mutable -> immutable, allowing zero copy
                // cloning.
                let message = message.freeze();

                // Now, send the message to all other peers
                for (addr, tx) in &self.state.lock().unwrap().peers {
                    // Don't send the message to ourselves
                    if *addr != self.addr {
                        // The send only fails if the rx half has been dropped,
                        // however this is impossible as the `tx` half will be
                        // removed from the map before the `rx` is dropped.
                        tx.unbounded_send(message.clone()).unwrap();
                    }
                }
            } else {
                // EOF was reached. The remote client has disconnected. There is
                // nothing more to do.
                return Ok(Async::Ready(()));
            }
        }

        // As always, it is important to not just return `NotReady` without
        // ensuring an inner future also returned `NotReady`.
        //
        // We know we got a `NotReady` from either `self.rx` or `self.messages`,
        // so the contract is respected.
        Ok(Async::NotReady)
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.state.lock().unwrap().peers.remove(&self.addr);
    }
}

impl Messages {
    /// Create a new `Messages` codec backed by the socket
    fn new(socket: TcpStream) -> Self {
        Messages {
            socket,
            rd: BytesMut::new(),
            wr: BytesMut::new(),
        }
    }

    /// Buffer a message.
    ///
    /// This writes the message to an internal buffer. Calls to `poll_flush`
    /// will attempt to flush this buffer to the socket.
    fn buffer(&mut self, message: &[u8]) {
        // Ensure the buffer has capacity. Ideally this would not be unbounded,
        // but to keep the example simple, we will not limit this.
        self.wr.reserve(message.len());

        // Push the message onto the end of the write buffer.
        //
        // The `put` function is from the `BufMut` trait.
        self.wr.put(message);
    }

    /// Flush the write buffer to the socket
    fn poll_flush(&mut self) -> Poll<(), io::Error> {
        // As long as there is buffered data to write, try to write it.
        while !self.wr.is_empty() {
            // Try to write some bytes to the socket
            let n = try_ready!(self.socket.poll_write(&self.wr));

            // As long as the wr is not empty, a successful write should
            // never write 0 bytes.
            assert!(n > 0);

            // This discards the first `n` bytes of the buffer.
            let _ = self.wr.split_to(n);
        }

        Ok(Async::Ready(()))
    }

    /// Read data from the socket.
    ///
    /// This only returns `Ready` when the socket has closed.
    fn fill_read_buf(&mut self) -> Poll<(), io::Error> {
        loop {
            // Ensure the read buffer has capacity.
            //
            // This might result in an internal allocation.
            self.rd.reserve(1024);

            // Read data into the buffer.
            let n = try_ready!(self.socket.read_buf(&mut self.rd));

            if n == 0 {
                return Ok(Async::Ready(()));
            }
        }
    }
}

impl Stream for Messages {
    type Item = BytesMut;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // First, read any new data that might have been received off the socket
        let sock_closed = self.fill_read_buf()?.is_ready();

        // TODO: instead of assuming lines, use serde to decode binary
        // message format. This will allow access to metadata which is
        // important for when a message has reached its target and for
        // cryptographic applicattions.

        // Now, try finding messages
        let pos = self
            .rd
            .windows(2)
            .enumerate()
            .find(|&(_, bytes)| bytes == b"\r\n")
            .map(|(i, _)| i);

        if let Some(pos) = pos {
            // Remove the message from the read buffer and set it to `message`.
            let mut message = self.rd.split_to(pos + 2);

            // Drop the trailing \r\n
            message.split_off(pos);

            // Return the message
            return Ok(Async::Ready(Some(message)));
        }

        if sock_closed {
            Ok(Async::Ready(None))
        } else {
            Ok(Async::NotReady)
        }
    }
}

/// Spawn a task to manage the socket.
///
/// This will read the first message from the socket to identify the client,
/// then add the client to the set of connected peers in the chat service.
fn process(socket: TcpStream, state: Arc<Mutex<Shared>>) {
    // Wrap the socket with the `Messages` codec that we wrote above.
    //
    // By doing this, we can operate at the message level instead of doing raw
    // byte manipulation.
    let messages = Messages::new(socket);

    let connection = Peer::new(state, messages).map_err(|e| {
        eprintln!("connection error = {:?}", e);
    });

    // Spawn the task. Internally, this submits the task to a thread pool.
    tokio::spawn(connection);
}

pub fn reactor() {
    // Create the shared state. This is how all the peers communicate.
    //
    // The server task will hold a handle to this. For every new client, the
    // `state` handle is cloned and passed into the task that processes the
    // client connection.
    let state = Arc::new(Mutex::new(Shared::new()));

    // Get IP address to use (default to 127.0.0.1).
    let mut addr = String::new();
    println!("Enter address to use (default: localhost)");
    let _ = std::io::stdin().read_line(&mut addr);
    let addr = addr
        .trim()
        .parse::<Ipv4Addr>()
        .unwrap_or(Ipv4Addr::LOCALHOST);

    // Get port number (default to 6142)
    let mut port = String::new();
    println!("Enter port to use (default: 6142)");
    let _ = std::io::stdin().read_line(&mut port);
    let port = port.trim().parse::<u16>().unwrap_or(6142);

    let addr = SocketAddr::new(IpAddr::V4(addr), port);

    // Bind a TCP listener to the socket address.
    //
    // Note that this is the Tokio TcpListener, which is fully async.
    let listener = TcpListener::bind(&addr).unwrap();

    // Get bootstrap node (optional)
    let mut bootstrap = String::new();
    println!("Enter bootstrap node address (optional)");
    let _ = std::io::stdin().read_line(&mut bootstrap);
    let bootstrap = bootstrap.trim().parse::<SocketAddr>();

    // Future for adding a bootstrap node.
    let state_clone = state.clone();
    let add_bootstrap = bootstrap
        .into_future()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
        .and_then(|addr| TcpStream::connect(&addr))
        .and_then(move |socket| {
            process(socket, state_clone);
            Ok(())
        }).map_err(|_| ());

    // The server task asynchronously iterates over and processes each
    // incoming connection.
    let server = listener
        .incoming()
        .for_each(move |socket| {
            // Spawn a task to process the connection
            process(socket, state.clone());
            Ok(())
        }).map_err(|err| {
            // All tasks must have an `Error` type of `()`. This forces error
            // handling and helps avoid silencing failures.
            //
            // In our example, we are only going to log the error to STDOUT.
            println!("accept error = {:?}", err);
        });

    println!("server running on {}", addr);

    // Start the Tokio runtime.
    //
    // This runs the given future, which first adds the bootstrap node (if any)
    // to the peers list. Then it starts the server.
    tokio::run(add_bootstrap.then(|_| server));
}
