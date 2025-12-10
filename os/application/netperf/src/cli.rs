use alloc::string::String;
use core::net::{IpAddr, Ipv4Addr};
use network::resolve_hostname;
use runtime::env;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Copy, Clone)]
pub struct Cli {
    pub mode: Mode,
    pub host: IpAddr,
    pub port: u16,
    pub protocol: Protocol,
    pub reverse: bool,
    pub interval_seconds: u32,
    pub duration_seconds: u32,
    pub json_output: bool,
    pub parallel_streams: u32,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub enum Mode {
    Server,
    Client,
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq)]
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
        let mut reverse = false;
        let mut interval_seconds: u32 = 1;
        let mut duration_seconds: u32 = 10;
        let mut json_output = false;
        let mut parallel_streams: u32 = 1;

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
                Some("-r") => {
                    args.next();
                    reverse = true;
                }
                Some("-i") => {
                    args.next();
                    match args.next().map(|arg| arg.parse::<u32>()) {
                        Some(Ok(arg)) => interval_seconds = arg,
                        _ => {
                            return Err("Wrong usage of option -i");
                        }
                    }
                }
                Some("-t") => {
                    args.next();
                    match args.next().map(|arg| arg.parse::<u32>()) {
                        Some(Ok(arg)) => duration_seconds = arg,
                        _ => {
                            return Err("Wrong usage of option -t");
                        }
                    }
                }
                Some("-P") => {
                    args.next();
                    match args.next().map(|arg| arg.parse::<u32>()) {
                        Some(Ok(arg)) if arg >= 1 => parallel_streams = arg,
                        _ => {
                            return Err("Wrong usage of option -P (must be >= 1)");
                        }
                    }
                }
                Some("--json") => {
                    args.next();
                    json_output = true;
                }
                Some(_) => return Err("Usage: netperf [-s|-c host] [options]"),
                None => break,
            }
        }

        if duration_seconds < interval_seconds {
            return Err("The duration must be at least as long as the interval");
        }

        Ok(Cli {
            mode,
            host,
            port,
            protocol,
            reverse,
            interval_seconds,
            duration_seconds,
            json_output,
            parallel_streams,
        })
    }
}
