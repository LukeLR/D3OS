use alloc::format;
use alloc::string::{String, ToString};
use core::iter::Peekable;
use core::net::{IpAddr, Ipv4Addr};
use network::resolve_hostname;
use runtime::env;
use runtime::env::Args;
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
    pub transfer_bytes: Option<u64>,
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
    pub fn parse() -> Result<Cli, String> {
        let mut args = env::args().peekable();
        // Skip the program name
        args.next();

        let mode = match args.peek().map(String::as_str) {
            Some("-s") => Mode::Server,
            Some("-c") => Mode::Client,
            _ => return Err("Usage: netperf [-s|-c host] [options]".to_string()),
        };
        args.next();

        let host = match args.peek() {
            Some(arg) if !arg.starts_with('-') => {
                let host_str = args.next().unwrap();

                resolve_hostname(&host_str)
                    .into_iter()
                    .next()
                    .ok_or_else(|| "Could not resolve host".to_string())?
            }
            _ => match mode {
                Mode::Server => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                Mode::Client => return Err("Usage: netperf [-s|-c host] [options]".to_string()),
            },
        };

        let mut cli = Cli {
            mode,
            host,
            port: 2000,
            protocol: Protocol::Tcp,
            reverse: false,
            interval_seconds: 1,
            duration_seconds: 10,
            transfer_bytes: None,
            json_output: false,
            parallel_streams: 1,
        };

        loop {
            match args.peek().map(String::as_str) {
                Some("-p") => {
                    let val = Self::parse_next(&mut args, "-p")?;
                    cli.port = val.parse().map_err(|_| "Invalid port number")?;
                }
                Some("-u") => {
                    args.next();
                    cli.protocol = Protocol::Udp;
                }
                Some("-r") => {
                    args.next();
                    cli.reverse = true;
                }
                Some("-i") => {
                    let val = Self::parse_next(&mut args, "-i")?;
                    cli.interval_seconds = val.parse().map_err(|_| "Invalid interval")?;
                }
                Some("-t") => {
                    let val = Self::parse_next(&mut args, "-t")?;
                    cli.duration_seconds = val.parse().map_err(|_| "Invalid duration")?;
                }
                Some("-n") => {
                    let val = Self::parse_next(&mut args, "-n")?;
                    cli.transfer_bytes = Some(Self::parse_bytes(&val).map_err(|e| format!("Option -n: {}", e))?);
                }
                Some("-P") => {
                    let val = Self::parse_next(&mut args, "-P")?;
                    let streams: u32 = val.parse().map_err(|_| "Invalid parallel streams")?;
                    if streams < 1 {
                        return Err("Parallel streams must be >= 1".to_string());
                    }
                    cli.parallel_streams = streams;
                }
                Some("--json") => {
                    args.next();
                    cli.json_output = true;
                }
                Some(_) => return Err("Usage: netperf [-s|-c host] [options]".to_string()),
                None => break,
            }
        }

        if cli.duration_seconds < cli.interval_seconds {
            return Err("The duration must be at least as long as the interval".to_string());
        }

        Ok(cli)
    }

    fn parse_next(args: &mut Peekable<Args>, option_name: &str) -> Result<String, String> {
        args.next();
        args.next().ok_or_else(|| format!("Missing value for option {}", option_name))
    }

    fn parse_bytes(input: &str) -> Result<u64, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Empty input".to_string());
        }

        // Check the last character to see if it's a suffix
        let last_char = input.chars().last().unwrap();

        let (num_str, multiplier) = if last_char.is_alphabetic() {
            // Split the number from the suffix
            let (num_str, _) = input.split_at(input.len() - last_char.len_utf8());

            // Determine multiplier (Base 2 for Volume)
            let mult = match last_char.to_ascii_uppercase() {
                'K' => 1024,
                'M' => 1024_u64.pow(2),
                'G' => 1024_u64.pow(3),
                'T' => 1024_u64.pow(4),
                _ => return Err(format!("Unknown suffix: {}", last_char)),
            };

            (num_str, mult)
        } else {
            // No suffix, treat as raw bytes
            (input, 1)
        };

        // Parse the numeric part
        match num_str.parse::<u64>() {
            Ok(val) => Ok(val * multiplier),
            Err(_) => Err("Could not parse number".to_string()),
        }
    }
}
