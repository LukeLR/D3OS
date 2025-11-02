use alloc::string::String;
use core::net::{IpAddr, Ipv4Addr};
use network::resolve_hostname;
use runtime::env;

pub struct Cli {
    pub mode: Mode,
    pub host: IpAddr,
    pub port: u16,
    pub protocol: Protocol,
}

pub enum Mode {
    Server,
    Client,
}

pub enum Protocol {
    Tcp,
    Udp,
}

impl Cli {
    pub fn parse() -> Result<Cli, &'static str> {
        let mut args = env::args().peekable();
        // Skip the program name
        args.next();

        let mode = match args.peek().map(String::as_str) {
            Some("-s") => Mode::Server,
            Some("-c") => Mode::Client,
            _ => return Err("Usage: netperf [-s|-c host] [options]"),
        };
        args.next();

        let host = match args.peek() {
            Some(arg) if !arg.starts_with('-') => {
                let host_str = args.next().unwrap();

                match resolve_hostname(&host_str).into_iter().next() {
                    Some(ip) => Some(ip),
                    None => return Err("Could not resolve host or parse IP"),
                }
            }
            _ => match mode {
                Mode::Server => Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                Mode::Client => return Err("Usage: netperf [-s|-c host] [options]"),
            },
        }
        .unwrap();

        let mut port = 2000;
        let mut protocol = Protocol::Tcp;
        loop {
            match args.peek().map(String::as_str) {
                Some("-p") => {
                    args.next();
                    match args.next().map(|arg| arg.parse::<u16>()) {
                        Some(Ok(arg)) => port = arg,
                        _ => {
                            return Err("Wrong usage of option -p");
                        }
                    }
                }
                Some("-u") => {
                    args.next();
                    protocol = Protocol::Udp;
                }
                Some(_) => return Err("Usage: netperf [-s|-c host] [options]"),
                None => break,
            }
        }

        Ok(Cli { mode, host, port, protocol })
    }
}
