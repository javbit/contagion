use docopt::Docopt;
use self::editor::Editor;
use std::path::Path;
use text_ui::term_ui::formatter::ConsoleLineFormatter;
use text_ui::term_ui::TermUI;
use futures::sync::mpsc;
use bytes::{BufMut, Bytes, BytesMut};


mod buffer;
mod editor;
mod formatter;
mod string_utils;
mod term_ui;
mod utils;


/// Shorthand for the transmit half of the message channel.
pub type Tx = mpsc::UnboundedSender<Bytes>;

/// Shorthand for the receive half of the message channel.
pub type Rx = mpsc::UnboundedReceiver<Bytes>;

pub fn start_ui(sender_channel: Tx, receiver_channel: Rx, filename: String, all_messages: String) {
    let editor = Editor::new_from_file(ConsoleLineFormatter::new(4), &Path::new(&filename.as_str()),
                                         sender_channel, receiver_channel, &Path::new(&all_messages.as_str()));

    // Initialize and start UI
    let mut ui = TermUI::new_from_editor(editor);

    ui.main_ui_loop();
}
