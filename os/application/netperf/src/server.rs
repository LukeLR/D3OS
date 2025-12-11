use crate::Results;
use crate::cli::Cli;
use crate::protocol::{ControlMsg, Coordinator, recv_msg, send_msg};
use core::net::SocketAddr;
use network::{NetworkError, TcpListener, TcpStream};
use terminal::println;

pub struct Server {
    control_channel: TcpStream,
}

impl Server {
    pub fn listen(config: Cli) -> Result<Server, NetworkError> {
        let listener = TcpListener::bind(SocketAddr::new(config.host, config.port))?;

        println!("-------------------------------------------");
        println!("Server listening on {}, port {}", config.host, config.port);
        println!("-------------------------------------------");

        let control_channel = listener.accept()?;

        println!(
            "Accepted connection from {} port {}",
            control_channel.peer_addr().ip(),
            control_channel.peer_addr().port()
        );

        Ok(Server { control_channel })
    }

    pub fn handshake(&self) -> Cli {
        let client_arguments = match recv_msg(&self.control_channel) {
            ControlMsg::CliArgs(cli) => cli,
            _ => panic!("wrong control message"),
        };

        send_msg(&self.control_channel, &ControlMsg::Ack);

        client_arguments
    }

    pub fn send_results(&self, results: Results) {
        send_msg(&self.control_channel, &ControlMsg::Results(results.summary, results.json));
    }

    pub fn signal_ready(&self) {
        send_msg(&self.control_channel, &ControlMsg::Ready);
    }

    pub fn wait_for_ready(&self) {
        match recv_msg(&self.control_channel) {
            ControlMsg::Ready => {}
            _ => panic!("expected ready signal from client"),
        }
    }
}

impl Coordinator for Server {
    fn send(&self, msg: &ControlMsg) {
        send_msg(&self.control_channel, msg);
    }

    fn recv(&self) -> ControlMsg {
        recv_msg(&self.control_channel)
    }

    fn local_addr(&self) -> SocketAddr {
        self.control_channel.local_addr()
    }

    fn remote_addr(&self) -> SocketAddr {
        self.control_channel.peer_addr()
    }

    fn is_server(&self) -> bool {
        true
    }
}
