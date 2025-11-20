use crate::cli::Cli;
use crate::protocol::{ControlMsg, recv_msg, send_msg};
use alloc::string::String;
use core::net::SocketAddr;
use concurrent::thread::sleep;
use network::{NetworkError, TcpListener, TcpStream};
use syscall::return_vals::Errno;
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

    pub fn send_results(&self, header: String, results: String) {
        send_msg(&self.control_channel, &ControlMsg::Results(header, results));
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

    pub fn get_client_address(&self) -> SocketAddr {
        self.control_channel.peer_addr()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        loop {
            // workaround: wait until the client closes the connection
            if let Err(NetworkError::Unknown(Errno::EINVALH)) = self.control_channel.read(&mut []) {
                break;
            }
            sleep(100);
        }
    }
}
