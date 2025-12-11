#![no_std]

extern crate alloc;
mod cli;
mod client;
mod protocol;
mod server;
mod stats;

use crate::protocol::Coordinator;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use cli::{Cli, Mode, Protocol};
use client::Client;
use concurrent::thread;
use core::fmt::{self, Display, Formatter};
use core::net::SocketAddr;
use core::sync::atomic::{AtomicBool, Ordering};
use network::{NetworkError, TcpListener, TcpStream, UdpSocket};
use protocol::{TCP_RECV_BUFFER_SIZE, TCP_SEND_MESSAGE_SIZE, UDP_RECV_BUFFER_SIZE, UDP_SEND_MESSAGE_SIZE};
use server::Server;
use spin::Mutex;
use stats::Stats;
use terminal::println;

// Static work queues for passing data to threads (since thread::create only takes fn())
static TCP_WORK_QUEUE: Mutex<VecDeque<TcpWorkItem>> = Mutex::new(VecDeque::new());
static UDP_SENDER_WORK_QUEUE: Mutex<VecDeque<UdpSenderWorkItem>> = Mutex::new(VecDeque::new());
static UDP_RECEIVER_WORK_QUEUE: Mutex<VecDeque<UdpReceiverWorkItem>> = Mutex::new(VecDeque::new());

struct TcpWorkItem {
    role: Role,
    stats: Arc<Stats>,
    socket: TcpStream,
    start_flag: Arc<AtomicBool>,
}

struct UdpSenderWorkItem {
    stats: Arc<Stats>,
    socket: UdpSocket,
    remote_addr: SocketAddr,
    start_flag: Arc<AtomicBool>,
}

struct UdpReceiverWorkItem {
    stats: Arc<Stats>,
    socket: Arc<UdpSocket>,
    start_flag: Arc<AtomicBool>,
}

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
    fn inverse(self) -> Self {
        match self {
            Self::Sender => Self::Receiver,
            Self::Receiver => Self::Sender,
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

struct Results {
    pub summary: String,
    pub json: String,
}

impl Results {
    fn new(stats: &Arc<Stats>) -> Self {
        Self {
            summary: stats.finalize_and_get_summary(),
            json: stats.stats_as_json(),
        }
    }
}

fn start_server(config: Cli) {
    loop {
        let server = Server::listen(config).expect("server err");
        let client_config = server.handshake();
        let local_addr = SocketAddr::new(config.host, config.port);

        let role = if client_config.reverse { Role::Sender } else { Role::Receiver };

        let results = match client_config.protocol {
            Protocol::Tcp => {
                if client_config.parallel_streams > 1 {
                    run_tcp_parallel(&server, role, client_config, local_addr)
                } else {
                    let listener = TcpListener::bind(local_addr).expect("failed to bind tcp socket");
                    let socket = listener.accept().expect("failed to accept tcp connection");

                    match role {
                        Role::Receiver => server.signal_ready(),
                        Role::Sender => server.wait_for_ready(),
                    }

                    run_tcp_single(role, socket, client_config)
                }
            }
            Protocol::Udp => {
                if client_config.parallel_streams > 1 {
                    run_udp_parallel(&server, role, client_config, local_addr)
                } else {
                    let remote = match role {
                        Role::Sender => Some(server.remote_addr()),
                        Role::Receiver => None,
                    };

                    match role {
                        Role::Receiver => server.signal_ready(),
                        Role::Sender => server.wait_for_ready(),
                    }

                    run_udp(role, local_addr, remote, client_config)
                }
            }
        };

        println!("- - - - - - - - - - - - - - - - - - - - - - -");
        println!("{}:", role);
        println!("{}", results.summary);

        server.send_results(results);
    }
}

fn start_client(config: Cli) {
    let client = Client::connect(config).expect("client err");
    client.handshake(config);

    let role = if config.reverse { Role::Receiver } else { Role::Sender };

    let results = match config.protocol {
        Protocol::Tcp => {
            if config.parallel_streams > 1 {
                run_tcp_parallel(&client, role, config, SocketAddr::new(config.host, config.port))
            } else {
                let socket = TcpStream::connect(SocketAddr::new(config.host, config.port)).expect("failed to connect to tcp socket");

                match role {
                    Role::Receiver => client.signal_ready(),
                    Role::Sender => client.wait_for_ready(),
                }

                run_tcp_single(role, socket, config)
            }
        }
        Protocol::Udp => {
            if config.parallel_streams > 1 {
                run_udp_parallel(&client, role, config, client.local_addr())
            } else {
                let local = client.local_addr();
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
        }
    };

    println!("- - - - - - - - - - - - - - - - - - - - - - - -");
    println!("{}:", role);
    println!("{}", results.summary);

    let server_results = client.receive_server_results();

    println!("{}:", role.inverse());
    println!("{}", server_results.summary);

    if config.json_output {
        // TODO: Write json data into a file
    }
}

fn run_tcp_single(role: Role, socket: TcpStream, config: Cli) -> Results {
    let stats = Arc::new(Stats::tcp(config.interval_seconds, config.duration_seconds));
    let thread_id = thread::current().map(|t| t.id()).unwrap_or(0);
    stats.register_thread(thread_id);
    println!("{}", stats.get_header());

    match role {
        Role::Sender => tcp_sender_loop(&stats, socket, thread_id),
        Role::Receiver => tcp_receiver_loop(&stats, socket, thread_id),
    }

    Results::new(&stats)
}

fn run_tcp_parallel<C: Coordinator>(coordinator: &C, role: Role, config: Cli, local_addr: SocketAddr) -> Results {
    let stats = Arc::new(Stats::tcp(config.interval_seconds, config.duration_seconds));
    let start_flag = Arc::new(AtomicBool::new(false));
    let threads = Arc::new(Mutex::new(Vec::new()));

    let sockets = if coordinator.is_server() {
        accept_tcp_streams(coordinator, config, local_addr)
    } else {
        connect_tcp_streams(coordinator, config)
    };

    for socket in sockets {
        TCP_WORK_QUEUE.lock().push_back(TcpWorkItem {
            role,
            stats: Arc::clone(&stats),
            socket,
            start_flag: Arc::clone(&start_flag),
        });

        if let Some(t) = thread::create(tcp_thread_entry) {
            threads.lock().push(t);
        }
    }

    match role {
        Role::Sender => coordinator.wait_for_start_benchmark(),
        Role::Receiver => coordinator.signal_start_benchmark(),
    }

    start_flag.store(true, Ordering::Release);
    join_threads(&threads);

    Results::new(&stats)
}

fn connect_tcp_streams<C: Coordinator>(coordinator: &C, config: Cli) -> Vec<TcpStream> {
    let mut sockets = Vec::with_capacity(config.parallel_streams as usize);

    for _ in 0..config.parallel_streams {
        coordinator.wait_for_stream_ready();
        let socket = TcpStream::connect(SocketAddr::new(config.host, config.port)).expect("failed to connect tcp stream");
        sockets.push(socket);
    }

    sockets
}

fn accept_tcp_streams<C: Coordinator>(coordinator: &C, config: Cli, local_addr: SocketAddr) -> Vec<TcpStream> {
    let mut sockets = Vec::with_capacity(config.parallel_streams as usize);

    for stream_id in 0..config.parallel_streams {
        let listener = TcpListener::bind(local_addr).expect("failed to bind tcp socket");
        coordinator.signal_stream_ready(stream_id);
        let socket = listener.accept().expect("failed to accept tcp connection");
        sockets.push(socket);
    }

    sockets
}

fn tcp_thread_entry() {
    let work_item = TCP_WORK_QUEUE.lock().pop_front().expect("no tcp work item");
    let thread_id = current_thread_id();
    work_item.stats.register_thread(thread_id);

    wait_for_start(&work_item.start_flag);
    let role = work_item.role;
    let stats = &work_item.stats;
    let socket = work_item.socket;

    match role {
        Role::Sender => tcp_sender_loop(stats, socket, thread_id),
        Role::Receiver => tcp_receiver_loop(stats, socket, thread_id),
    }
}

fn tcp_receiver_loop(stats: &Arc<Stats>, socket: TcpStream, thread_id: usize) {
    let mut buf = vec![0; TCP_RECV_BUFFER_SIZE];

    while !stats.has_total_time_elapsed() {
        if let Ok(true) = socket.can_recv() {
            match socket.read(&mut buf) {
                Ok(len) => {
                    if len > 0 {
                        stats.track(thread_id, len, &buf);
                    }
                }
                Err(err) => {
                    if !handle_network_error(err, "receive message") {
                        break;
                    }
                }
            }
        } else {
            thread::sleep(30);
        }

        stats.print_interval_info();
    }
}

fn tcp_sender_loop(stats: &Arc<Stats>, socket: TcpStream, thread_id: usize) {
    let message = vec![0; TCP_SEND_MESSAGE_SIZE];

    while !stats.has_total_time_elapsed() {
        if let Ok(true) = socket.can_send() {
            match socket.write(&message) {
                Ok(len) => stats.track(thread_id, len, &[]),
                Err(err) => {
                    if !handle_network_error(err, "send message") {
                        break;
                    }
                }
            };
        } else {
            thread::sleep(30)
        }

        stats.print_interval_info();
    }
}

fn wait_for_start(flag: &AtomicBool) {
    while !flag.load(Ordering::Acquire) {
        thread::switch();
    }
}

fn join_threads(threads: &Arc<Mutex<Vec<thread::Thread>>>) {
    let threads_vec: Vec<thread::Thread> = {
        let mut lock = threads.lock();
        core::mem::take(&mut *lock)
    };
    for t in threads_vec {
        t.join();
    }
}

fn run_udp(role: Role, local_addr: SocketAddr, remote: Option<SocketAddr>, config: Cli) -> Results {
    match role {
        Role::Sender => start_udp_sender(local_addr, remote.expect("remote addr required for UDP sender"), config),
        Role::Receiver => start_udp_receiver(local_addr, config),
    }
}

fn run_udp_parallel<C: Coordinator>(coordinator: &C, role: Role, config: Cli, local_addr: SocketAddr) -> Results {
    let stats = Arc::new(Stats::udp(config.interval_seconds, config.duration_seconds, role));
    let start_flag = Arc::new(AtomicBool::new(false));
    let threads = Arc::new(Mutex::new(Vec::new()));

    match role {
        Role::Sender => {
            let remote_addr = coordinator.remote_addr();

            for _ in 0..config.parallel_streams {
                let socket = UdpSocket::bind(local_addr).expect("failed to bind udp socket");

                UDP_SENDER_WORK_QUEUE.lock().push_back(UdpSenderWorkItem {
                    stats: Arc::clone(&stats),
                    socket,
                    remote_addr,
                    start_flag: Arc::clone(&start_flag),
                });

                if let Some(t) = thread::create(udp_sender_thread_entry) {
                    threads.lock().push(t);
                }
            }

            coordinator.wait_for_start_benchmark();
        }
        Role::Receiver => {
            let socket = Arc::new(UdpSocket::bind(local_addr).expect("failed to bind udp socket"));

            for _ in 0..config.parallel_streams {
                UDP_RECEIVER_WORK_QUEUE.lock().push_back(UdpReceiverWorkItem {
                    stats: Arc::clone(&stats),
                    socket: Arc::clone(&socket),
                    start_flag: Arc::clone(&start_flag),
                });

                if let Some(t) = thread::create(udp_receiver_thread_entry) {
                    threads.lock().push(t);
                }
            }

            coordinator.signal_start_benchmark();
        }
    }

    start_flag.store(true, Ordering::Release);
    join_threads(&threads);

    Results::new(&stats)
}

fn udp_receiver_thread_entry() {
    let work_item = UDP_RECEIVER_WORK_QUEUE.lock().pop_front().expect("no udp receiver work item");
    let thread_id = current_thread_id();
    work_item.stats.register_thread(thread_id);

    wait_for_start(&work_item.start_flag);
    udp_receiver_loop(&work_item.stats, &work_item.socket, thread_id);
}

fn udp_sender_thread_entry() {
    let work_item = UDP_SENDER_WORK_QUEUE.lock().pop_front().expect("no udp sender work item");
    let thread_id = current_thread_id();
    work_item.stats.register_thread(thread_id);

    wait_for_start(&work_item.start_flag);
    udp_sender_loop(&work_item.stats, work_item.socket, work_item.remote_addr, thread_id);
}

fn start_udp_receiver(local_addr: SocketAddr, config: Cli) -> Results {
    let socket = Arc::new(UdpSocket::bind(local_addr).expect("failed to open socket"));
    let stats = Arc::new(Stats::udp(config.interval_seconds, config.duration_seconds, Role::Receiver));
    let thread_id = current_thread_id();
    stats.register_thread(thread_id);
    println!("{}", stats.get_header());

    udp_receiver_loop(&stats, &socket, thread_id);

    Results::new(&stats)
}

fn start_udp_sender(local_addr: SocketAddr, remote_addr: SocketAddr, config: Cli) -> Results {
    let socket = UdpSocket::bind(local_addr).expect("failed to open socket");
    let stats = Arc::new(Stats::udp(config.interval_seconds, config.duration_seconds, Role::Sender));
    let thread_id = current_thread_id();
    stats.register_thread(thread_id);
    println!("{}", stats.get_header());

    udp_sender_loop(&stats, socket, remote_addr, thread_id);

    Results::new(&stats)
}

fn udp_receiver_loop(stats: &Arc<Stats>, socket: &Arc<UdpSocket>, thread_id: usize) {
    let mut buf = vec![0; UDP_RECV_BUFFER_SIZE];

    while !stats.has_total_time_elapsed() {
        if let Ok(true) = socket.can_recv() {
            match socket.recv_from(&mut buf) {
                Ok((len, _addr)) => {
                    if len > 0 {
                        stats.track(thread_id, len, &buf);
                    }
                }
                Err(err) => {
                    if !handle_network_error(err, "receive message") {
                        break;
                    }
                }
            }
        } else {
            thread::sleep(30)
        }

        stats.print_interval_info();
    }
}

fn udp_sender_loop(stats: &Arc<Stats>, socket: UdpSocket, remote_addr: SocketAddr, thread_id: usize) {
    let mut message = vec![0; UDP_SEND_MESSAGE_SIZE];
    let mut seq_num: u64 = 0;

    while !stats.has_total_time_elapsed() {
        if let Ok(true) = socket.can_send() {
            message[..8].copy_from_slice(&seq_num.to_le_bytes());
            message[8..16].copy_from_slice(&time::systime().as_seconds_f64().to_le_bytes());

            match socket.send_to(&message, remote_addr) {
                Ok(len) => {
                    stats.track(thread_id, len, &[]);
                    seq_num += 1;
                }
                Err(err) => {
                    if !handle_network_error(err, "send message") {
                        break;
                    }
                }
            };
        } else {
            thread::sleep(30)
        }

        stats.print_interval_info();
    }
}

fn current_thread_id() -> usize {
    thread::current().map(|t| t.id()).unwrap_or(0)
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
