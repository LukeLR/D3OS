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
    Results(String, String, String),
    Ack,
    Ready,
}

/// Blocks until the message is sent.
pub fn send_msg(stream: &TcpStream, msg: &ControlMsg) {
    let buf: Vec<u8> = to_allocvec(msg).expect("unable to allocate buffer");
    let len_bytes = (buf.len() as u32).to_be_bytes();

    write_all(stream, &len_bytes);
    write_all(stream, &buf);
}

/// Blocks until a message is received.
pub fn recv_msg(stream: &TcpStream) -> ControlMsg {
    let mut len_buf = [0u8; 4];
    read_exact(stream, &mut len_buf);

    let len = u32::from_be_bytes(len_buf) as usize;

    let mut payload_buf = vec![0u8; len];
    read_exact(stream, &mut payload_buf);

    from_bytes(&payload_buf).expect("error receiving control message")
}

/// Continues reading until the buffer is filled.
fn read_exact(stream: &TcpStream, buf: &mut [u8]) {
    let mut received = 0;

    while received < buf.len() {
        if let Ok(true) = stream.can_recv() {
            match stream.read(&mut buf[received..]) {
                Ok(n) => received += n,
                Err(_) => panic!("stream read error"),
            }
        }
    }
}

/// Continues writing until the buffer is completely sent.
fn write_all(stream: &TcpStream, buf: &[u8]) {
    let mut sent = 0;

    while sent < buf.len() {
        if let Ok(true) = stream.can_send() {
            match stream.write(&buf[sent..]) {
                Ok(n) => sent += n,
                Err(_) => panic!("stream write error"),
            }
        }
    }
}
