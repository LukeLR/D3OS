use crate::cli::Cli;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use network::TcpStream;
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};

// Message buffer sizes
pub const TCP_RECV_BUFFER_SIZE: usize = 65535;
pub const TCP_SEND_MESSAGE_SIZE: usize = 8192;
pub const UDP_RECV_BUFFER_SIZE: usize = 65535;
pub const UDP_SEND_MESSAGE_SIZE: usize = 1472;

#[derive(Serialize, Deserialize)]
pub enum ControlMsg {
    CliArgs(Cli),
    Results(String, String),
    Ack,
    Ready,
}

/// Blocks until the message is sent.
pub fn send_msg(stream: &TcpStream, msg: &ControlMsg) {
    loop {
        if let Ok(true) = stream.can_send() {
            let buf: Vec<u8> = to_allocvec(msg).expect("unable to allocate buffer");

            let len_bytes = (buf.len() as u32).to_be_bytes();

            stream.write(&len_bytes).expect("error sending control message");
            stream.write(&buf).expect("error sending control message");
            break;
        }
    }
}

/// Blocks until a message is received.
pub fn recv_msg(stream: &TcpStream) -> ControlMsg {
    loop {
        if let Ok(true) = stream.can_recv() {
            let mut len_buf = [0u8; 4];

            stream.read(&mut len_buf).expect("error receiving control message");

            let len = u32::from_be_bytes(len_buf) as usize;
            let mut payload_buf = vec![0u8; len];

            stream.read(&mut payload_buf).expect("error receiving control message");
            return from_bytes(&payload_buf).expect("error receiving control message");
        }
    }
}
