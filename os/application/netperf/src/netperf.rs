#![no_std]

extern crate alloc;
mod cli;
mod client;
mod protocol;
mod server;
mod stats;

use crate::cli::{Cli, Mode, Protocol};
use crate::client::Client;
use crate::protocol::{TCP_RECV_BUFFER_SIZE, TCP_SEND_MESSAGE_SIZE, UDP_RECV_BUFFER_SIZE, UDP_SEND_MESSAGE_SIZE};
use crate::server::Server;
use crate::stats::Stats;
use alloc::string::String;
use alloc::vec;
use core::fmt;
use core::fmt::{Display, Formatter};
use core::net::SocketAddr;
use network::{NetworkError, TcpListener, TcpStream, UdpSocket};
#[allow(unused_imports)]
use runtime::*;
use terminal::println;

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

#[derive(Copy, Clone)]
enum Role {
    Sender,
    Receiver,
}

impl Role {
    pub fn inverse(&self) -> Role {
        match self {
            Role::Sender => Role::Receiver,
            Role::Receiver => Role::Sender,
        }
    }
}

impl Display for Role {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Role::Sender => write!(f, "Sender"),
            Role::Receiver => write!(f, "Receiver"),
        }
    }
}

fn start_server(config: Cli) {
    loop {
        // TODO: Handle errors
        let server = Server::listen(config).expect("server err");
        let client_config = server.handshake();
        let local_addr = SocketAddr::new(config.host, config.port);

        let role = if client_config.reverse { Role::Sender } else { Role::Receiver };

        let (header, summary) = match client_config.protocol {
            Protocol::Tcp => {
                let listener = TcpListener::bind(local_addr).expect("failed to bind tcp socket");
                let socket = listener.accept().expect("failed to accept tcp connection");

                match role {
                    Role::Receiver => server.signal_ready(),
                    Role::Sender => server.wait_for_ready(),
                }

                run_tcp(role, socket, client_config)
            }
            Protocol::Udp => {
                let remote = match role {
                    Role::Sender => Some(server.get_client_address()),
                    Role::Receiver => None,
                };

                match role {
                    Role::Receiver => server.signal_ready(),
                    Role::Sender => server.wait_for_ready(),
                }

                run_udp(role, local_addr, remote, client_config)
            }
        };

        println!("- - - - - - - - - - - - - - - - - - - - - - -");
        println!("{}:", role);
        println!("{}", summary);

        server.send_results(header, summary);
    }
}

fn start_client(config: Cli) {
    // TODO: Handle errors
    let client = Client::connect(config).expect("client err");
    client.handshake(config);

    let role = if config.reverse { Role::Receiver } else { Role::Sender };

    let (_, local_summary) = match config.protocol {
        Protocol::Tcp => {
            let socket = TcpStream::connect(SocketAddr::new(config.host, config.port)).expect("failed to connect to tcp socket");

            match role {
                Role::Receiver => client.signal_ready(),
                Role::Sender => client.wait_for_ready(),
            }

            run_tcp(role, socket, config)
        }
        Protocol::Udp => {
            let local = client.get_local_addr();
            let remote = match role {
                Role::Sender => Some(SocketAddr::new(config.host, config.port)),
                Role::Receiver => None,
            };

            match role {
                Role::Receiver => client.signal_ready(),
                Role::Sender => client.wait_for_ready(),
            }

            run_udp(role, local, remote, config)
        }
    };

    println!("- - - - - - - - - - - - - - - - - - - - - - - -");
    println!("{}:", role);
    println!("{}", local_summary);

    let (server_header, server_summary) = client.receive_server_results();

    println!("{}:", role.inverse());
    println!("{}", server_header);
    println!("{}", server_summary);
}

fn run_tcp(role: Role, socket: TcpStream, config: Cli) -> (String, String) {
    match role {
        Role::Sender => start_tcp_sender(socket, config),
        Role::Receiver => start_tcp_receiver(socket, config),
    }
}

fn run_udp(role: Role, local_addr: SocketAddr, remote: Option<SocketAddr>, config: Cli) -> (String, String) {
    match role {
        Role::Sender => start_udp_sender(local_addr, remote.expect("remote addr required for UDP sender"), config),
        Role::Receiver => start_udp_receiver(local_addr, config),
    }
}

fn start_tcp_receiver(socket: TcpStream, config: Cli) -> (String, String) {
    let mut buf = vec![0; TCP_RECV_BUFFER_SIZE];

    let mut tracker = Stats::tcp(config.interval_seconds, config.duration_seconds);
    println!("{}", tracker.get_header());

    while !tracker.has_total_time_elapsed() {
        if let Ok(true) = socket.can_recv() {
            match socket.read(&mut buf) {
                Ok(len) => {
                    if len > 0 {
                        tracker.track(len, &buf);
                    }
                }
                Err(err) => {
                    if !handle_network_error(err, "receive message") {
                        break;
                    }
                }
            }
        }

        tracker.print_interval_info();
    }

    (tracker.get_header(), tracker.finalize_and_get_summary())
}

fn start_tcp_sender(socket: TcpStream, config: Cli) -> (String, String) {
    let message = vec![0; TCP_SEND_MESSAGE_SIZE];

    let mut tracker = Stats::tcp(config.interval_seconds, config.duration_seconds);
    println!("{}", tracker.get_header());

    while !tracker.has_total_time_elapsed() {
        if let Ok(true) = socket.can_send() {
            match socket.write(&message) {
                Ok(len) => tracker.track(len, &[]),
                Err(err) => {
                    if !handle_network_error(err, "send message") {
                        break;
                    }
                }
            };
        }

        tracker.print_interval_info();
    }

    (tracker.get_header(), tracker.finalize_and_get_summary())
}

fn start_udp_sender(local_addr: SocketAddr, remote_addr: SocketAddr, config: Cli) -> (String, String) {
    let socket = UdpSocket::bind(local_addr).expect("failed to open socket");
    let mut message = vec![0; UDP_SEND_MESSAGE_SIZE];
    let mut seq_num: u64 = 0;

    let mut tracker = Stats::udp(config.interval_seconds, config.duration_seconds, Role::Sender);
    println!("{}", tracker.get_header());

    while !tracker.has_total_time_elapsed() {
        if let Ok(true) = socket.can_send() {
            message[..8].copy_from_slice(&seq_num.to_le_bytes());
            message[8..16].copy_from_slice(&time::systime().as_seconds_f64().to_le_bytes());

            match socket.send_to(&message, remote_addr) {
                Ok(len) => {
                    tracker.track(len, &[]);
                    seq_num += 1;
                }
                Err(err) => {
                    if !handle_network_error(err, "send message") {
                        break;
                    }
                }
            };
        }

        tracker.print_interval_info();
    }

    (tracker.get_header(), tracker.finalize_and_get_summary())
}

fn start_udp_receiver(local_addr: SocketAddr, config: Cli) -> (String, String) {
    let socket = UdpSocket::bind(local_addr).expect("failed to open socket");
    let mut buf = vec![0; UDP_RECV_BUFFER_SIZE];

    let mut tracker = Stats::udp(config.interval_seconds, config.duration_seconds, Role::Receiver);
    println!("{}", tracker.get_header());

    while !tracker.has_total_time_elapsed() {
        if let Ok(true) = socket.can_recv() {
            match socket.recv_from(&mut buf) {
                Ok((len, _addr)) => {
                    if len > 0 {
                        tracker.track(len, &buf);
                    }
                }
                Err(err) => {
                    if !handle_network_error(err, "receive message") {
                        break;
                    }
                }
            }
        }

        tracker.print_interval_info();
    }

    (tracker.get_header(), tracker.finalize_and_get_summary())
}

fn handle_network_error(err: NetworkError, operation: &str) -> bool {
    match err {
        NetworkError::DeviceBusy => true,
        NetworkError::InvalidAddress => {
            println!("Failed to {}: Invalid address.", operation);
            false
        }
        NetworkError::Unknown(_) => {
            println!("Failed to {}.", operation);
            false
        }
    }
}
