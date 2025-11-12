use crate::cli::Cli;
use crate::protocol::{recv_msg, send_msg, ControlMsg};
use alloc::string::String;
use core::net::SocketAddr;
use network::{NetworkError, TcpStream};
use terminal::println;

pub struct Client {
    control_channel: TcpStream,
}

impl Client {
    pub fn connect(config: Cli) -> Result<Client, NetworkError> {
        let control_channel = TcpStream::connect(SocketAddr::new(config.host, config.port))?;

        println!("-------------------------------------------");
        println!("Connected to {} on port {}", config.host, config.port);
        println!("-------------------------------------------");

        Ok(Client { control_channel })
    }

    pub fn handshake(&self, config: Cli) {
        send_msg(&self.control_channel, &ControlMsg::CliArgs(config));

        match recv_msg(&self.control_channel) {
            ControlMsg::Ack => {}
            _ => panic!("handshake failed"),
        }
    }

    pub fn receive_server_results(&self) -> (String, String) {
        match recv_msg(&self.control_channel) {
            ControlMsg::Results(header, summary) => (header, summary),
            _ => panic!("expected results from server"),
        }
    }

    pub fn wait_for_ready(&self) {
        match recv_msg(&self.control_channel) {
            ControlMsg::Ready => {}
            _ => panic!("expected ready signal from server"),
        }
    }

    pub fn signal_ready(&self) {
        send_msg(&self.control_channel, &ControlMsg::Ready);
    }
    
    pub fn get_local_addr(&self) -> SocketAddr {
        self.control_channel.local_addr()
    }
}
