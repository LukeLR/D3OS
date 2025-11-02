#![no_std]

extern crate alloc;
mod cli;
mod stats;

use crate::cli::{Cli, Mode, Protocol};
use crate::stats::StatsTracker;
use alloc::vec;
use concurrent::thread;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use network::{NetworkError, TcpListener, TcpStream, UdpSocket};
#[allow(unused_imports)]
use runtime::*;
use terminal::{print, println};

#[unsafe(no_mangle)]
pub fn main() {
    let cli = Cli::parse();

    if let Err(message) = cli {
        println!("{}", message);
        return;
    }

    let cli = cli.unwrap();

    match cli.mode {
        Mode::Server => start_server(cli),
        Mode::Client => start_client(cli),
    }
}

fn start_server(cli: Cli) {
    match cli.protocol {
        Protocol::Tcp => start_server_tcp(cli),
        Protocol::Udp => start_server_udp(cli),
    }
}

fn start_server_udp(cli: Cli) {
    let addr = cli.host;
    let port = cli.port;

    let socket = UdpSocket::bind(SocketAddr::new(addr, port)).expect("failed to open socket");

    println!("Opened UDP socket on {}, port {}", addr, port);
    stats::print_header();

    let mut buf = vec![0; 65535];
    let mut tracker = StatsTracker::new(1);
    let mut transmission_started = false;

    loop {
        if let Ok(true) = socket.can_recv() {
            let (len, _addr) = socket.recv_from(&mut buf).expect("failed to read from socket");
            if len > 0 {
                tracker.add_bytes(len);
            }

            transmission_started = true;
        }

        if transmission_started {
            tracker.check_and_print_report();
        }
    }
}

fn start_server_tcp(cli: Cli) {
    let addr = cli.host;
    let port = cli.port;

    let listener = TcpListener::bind(SocketAddr::new(addr, port)).expect("failed to open socket");

    println!("Server listening on {}, port {}", addr, port);

    let socket = listener.accept().expect("failed to accept connection");

    println!("Accepted connection from {}", socket.peer_addr());
    stats::print_header();

    let mut buf = vec![0; 65535];
    let mut tracker = StatsTracker::new(1);

    loop {
        if let Ok(true) = socket.can_recv() {
            let len = socket.read(&mut buf).expect("failed to read from socket");
            if len > 0 {
                tracker.add_bytes(len);
            }
        }

        tracker.check_and_print_report();
    }
}

fn start_client(cli: Cli) {
    match cli.protocol {
        Protocol::Tcp => start_client_tcp(cli),
        Protocol::Udp => start_client_udp(cli),
    }
}

fn start_client_udp(cli: Cli) {
    let remote_addr = cli.host;
    let remote_port = cli.port;
    let remote_socket = SocketAddr::new(remote_addr, remote_port);

    let socket = UdpSocket::bind(
        // TODO: Use random port
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 2000),
    )
    .expect("failed to open socket");

    println!("Starting to send to {} on port {}", remote_addr, remote_port);

    let message = vec![0; 1472];
    let mut tracker = StatsTracker::new(1);

    loop {
        if let Ok(true) = socket.can_send() {
            match socket.send_to(&message, remote_socket) {
                Ok(len) => tracker.add_bytes(len),
                Err(NetworkError::DeviceBusy) => continue,
                Err(NetworkError::InvalidAddress) => {
                    println!("Failed to send message: Invalid address.");
                    break;
                }
                Err(NetworkError::Unknown(_)) => {
                    println!("Failed to send message.");
                    break;
                }
            };
        }

        tracker.check_and_print_report();
    }
}

fn start_client_tcp(cli: Cli) {
    let remote_addr = cli.host;
    let remote_port = cli.port;

    let socket = TcpStream::connect(SocketAddr::new(remote_addr, remote_port)).expect("failed to connect to socket");

    println!("Starting to send to {} on port {}", remote_addr, remote_port);

    let message = vec![0; 8192];
    let mut tracker = StatsTracker::new(1);

    loop {
        // Just send a bunch of zeros
        if let Ok(true) = socket.can_send() {
            match socket.write(&message) {
                Ok(len) => tracker.add_bytes(len),
                Err(NetworkError::DeviceBusy) => continue,
                Err(NetworkError::InvalidAddress) => {
                    println!("Failed to send message: Invalid address.");
                    break;
                }
                Err(NetworkError::Unknown(_)) => {
                    println!("Failed to send message.");
                    break;
                }
            };
        }

        tracker.check_and_print_report();
    }
}
