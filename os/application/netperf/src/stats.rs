use core::fmt::Write;
extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use chrono::TimeDelta;
use spin::Mutex;
use terminal::println;
use crate::cli::Protocol;
use crate::Role;

const BITS_PER_BYTE: f64 = 8.0;
const BYTES_PER_MEGABYTE: f64 = 1024.0 * 1024.0;
const BITS_PER_MEGABIT: f64 = 1_000_000.0;

const COL_SEP: &str = " ";

trait Column {
    fn header(&self) -> &'static str;
    fn width(&self) -> usize;
    fn on_track(&mut self, _bytes: usize, _buf: &[u8]) {}
    fn measure_interval(&self, interval: IntervalInfo) -> Metric;
    fn interval_reset(&mut self);
    fn measure_overall(&self, total_duration_s: f64) -> Metric;
}

enum Metric {
    Bytes { total: u64 },
    Speed { mbps: f64 },
    Loss { received: u64, expected: u64, loss_pct: f64 },
    Jitter { ms: f64 },
    Id { id: usize },
    Interval { start_s: f64, end_s: f64 },
}

impl Metric {
    fn to_console_string(&self) -> String {
        match self {
            Metric::Bytes { total } => {
                alloc::format!("{:>6.2} MBytes", (*total as f64) / BYTES_PER_MEGABYTE)
            }
            Metric::Speed { mbps } => alloc::format!("{:>6.2} Mbits/sec", mbps),
            Metric::Loss { received, expected, loss_pct } => {
                let lost = expected.saturating_sub(*received);
                alloc::format!("{}/{} ({:.2}%)", lost, expected, loss_pct)
            }
            Metric::Jitter { ms } => alloc::format!("{:.3} ms", ms),
            Metric::Id { id } => {
                if *id == usize::MAX {
                    String::from("[SUM]")
                } else {
                    alloc::format!("[{}]", id)
                }
            }
            Metric::Interval { start_s, end_s } => alloc::format!("{:>4.2}-{:>4.2} sec", start_s, end_s),
        }
    }

    fn to_json(&self) -> String {
        match self {
            Metric::Bytes { total } => alloc::format!("\"bytes\": {}", total),
            Metric::Speed { mbps } => alloc::format!("\"bitrate_mbps\": {:.4}", mbps),
            Metric::Loss { received, expected, loss_pct } => {
                let lost = expected.saturating_sub(*received);
                alloc::format!(
                    "\"packets_received\": {}, \"packets_expected\": {}, \"packets_lost\": {}, \"loss_percent\": {:.4}",
                    received,
                    expected,
                    lost,
                    loss_pct
                )
            }
            Metric::Jitter { ms } => alloc::format!("\"jitter_ms\": {:.4}", ms),
            Metric::Id { id } => alloc::format!("\"thread_id\": {}", id),
            Metric::Interval { start_s, end_s } => alloc::format!("\"start\": {:.2}, \"end\": {:.2}", start_s, end_s),
        }
    }
}

struct StatsTracker {
    columns: Vec<Box<dyn Column + Send>>,
    interval_json_data: Vec<String>,
}

impl StatsTracker {
    fn new(columns: Vec<Box<dyn Column + Send>>) -> Self {
        Self {
            columns,
            interval_json_data: Vec::new(),
        }
    }

    fn get_header(&self) -> String {
        let mut line = String::new();

        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                line.push_str(COL_SEP);
            }

            let _ = write!(line, "{:>width$}", c.header(), width = c.width());
        }

        line
    }

    fn track(&mut self, bytes: usize, buf: &[u8]) {
        for c in &mut self.columns {
            c.on_track(bytes, buf);
        }
    }

    fn build_interval_string(&mut self, info: IntervalInfo) -> String {
        let mut line = String::new();
        let mut json = String::new();
        let _ = write!(json, "{{ \"seconds\": {:.2}", info.elapsed_seconds);

        for (i, column) in self.columns.iter_mut().enumerate() {
            let measurement = column.measure_interval(info);

            if i > 0 {
                line.push_str(COL_SEP);
            }

            let _ = write!(line, "{:>width$}", measurement.to_console_string(), width = column.width());
            let _ = write!(json, ", {}", measurement.to_json());

            column.interval_reset();
        }

        json.push_str(" }");
        self.interval_json_data.push(json);

        line
    }

    fn print_interval_info(&mut self, info: IntervalInfo) {
        println!("{}", self.build_interval_string(info));
    }

    fn build_summary(&mut self, total_duration_s: f64) -> String {
        let mut line = String::new();

        for (i, c) in self.columns.iter().enumerate() {
            let measurement = c.measure_overall(total_duration_s);

            if i > 0 {
                line.push_str(COL_SEP);
            }

            let _ = write!(line, "{:>width$}", measurement.to_console_string(), width = c.width());
        }

        line
    }

    fn get_json(&self, total_duration_s: f64) -> String {
        let mut json = String::new();

        let _ = write!(json, "{{ \"summary\": {{ \"duration_seconds\": {:.4}", total_duration_s);

        for c in &self.columns {
            let _ = write!(json, ", {}", c.measure_overall(total_duration_s).to_json());
        }

        json.push_str(" }, \"intervals\": [");

        for (i, interval) in self.interval_json_data.iter().enumerate() {
            if i > 0 {
                json.push_str(", ");
            }

            json.push_str(interval);
        }

        json.push_str("] }");
        json
    }
}

#[derive(Copy, Clone)]
struct IntervalInfo {
    elapsed_seconds: f64,
    interval_start_s: f64,
    interval_end_s: f64,
}

struct ReportInterval {
    last_report_time: TimeDelta,
    start_time: TimeDelta,
    report_interval: TimeDelta,
    total_duration: TimeDelta,
}

impl ReportInterval {
    fn new(interval_seconds: u32, total_time_seconds: u32) -> Self {
        let now = time::systime();
        Self {
            last_report_time: now,
            start_time: now,
            report_interval: TimeDelta::seconds(interval_seconds as i64),
            total_duration: TimeDelta::seconds(total_time_seconds as i64),
        }
    }

    fn check(&mut self) -> Option<IntervalInfo> {
        let current_time = time::systime();
        let elapsed = current_time - self.last_report_time;

        if elapsed >= self.report_interval {
            let elapsed_seconds = elapsed.as_seconds_f64();

            let info = IntervalInfo {
                elapsed_seconds,
                interval_start_s: (self.last_report_time - self.start_time).as_seconds_f64(),
                interval_end_s: (current_time - self.start_time).as_seconds_f64(),
            };

            self.last_report_time = current_time;
            return Some(info);
        }

        None
    }

    fn total_duration_s(&self) -> f64 {
        self.total_duration.as_seconds_f64()
    }

    fn clamp_now_to_end(&self) -> TimeDelta {
        let now = time::systime();
        let end = self.start_time + self.total_duration;
        if now > end { end } else { now }
    }

    /// Finalize any pending partial interval and advance the last_report_time.
    fn finalize_pending_interval(&mut self) -> Option<IntervalInfo> {
        let current_time = self.clamp_now_to_end();

        if current_time <= self.last_report_time {
            return None;
        }

        let elapsed = current_time - self.last_report_time;

        let info = IntervalInfo {
            elapsed_seconds: elapsed.as_seconds_f64(),
            interval_start_s: (self.last_report_time - self.start_time).as_seconds_f64(),
            interval_end_s: (current_time - self.start_time).as_seconds_f64(),
        };

        self.last_report_time = current_time;
        Some(info)
    }

    fn has_total_time_elapsed(&self) -> bool {
        time::systime() - self.start_time >= self.total_duration
    }
}

struct IntervalColumn;

impl IntervalColumn {
    fn new() -> Self {
        Self {}
    }
}

impl Column for IntervalColumn {
    fn header(&self) -> &'static str {
        "Interval"
    }

    fn width(&self) -> usize {
        18
    }

    fn measure_interval(&self, interval: IntervalInfo) -> Metric {
        Metric::Interval {
            start_s: interval.interval_start_s,
            end_s: interval.interval_end_s,
        }
    }

    fn interval_reset(&mut self) {
        // ignore
    }

    fn measure_overall(&self, total_duration_s: f64) -> Metric {
        Metric::Interval {
            start_s: 0f64,
            end_s: total_duration_s,
        }
    }
}

struct IdColumn {
    id: usize,
}

impl IdColumn {
    fn new(id: usize) -> Self {
        Self { id }
    }
}

impl Column for IdColumn {
    fn header(&self) -> &'static str {
        "[ID]"
    }

    fn width(&self) -> usize {
        5
    }

    fn measure_interval(&self, _: IntervalInfo) -> Metric {
        Metric::Id { id: self.id }
    }

    fn interval_reset(&mut self) {
        // ignore
    }

    fn measure_overall(&self, _total_duration_s: f64) -> Metric {
        Metric::Id { id: self.id }
    }
}

struct TransferColumn {
    bytes_interval: u64,
    total_bytes: u64,
}

impl TransferColumn {
    fn new() -> Self {
        Self {
            bytes_interval: 0,
            total_bytes: 0,
        }
    }
}

impl Column for TransferColumn {
    fn header(&self) -> &'static str {
        "Transfer"
    }
    fn width(&self) -> usize {
        16
    }

    fn on_track(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }

    fn measure_interval(&self, _: IntervalInfo) -> Metric {
        Metric::Bytes { total: self.bytes_interval }
    }

    fn interval_reset(&mut self) {
        self.bytes_interval = 0;
    }

    fn measure_overall(&self, _total_duration_s: f64) -> Metric {
        Metric::Bytes { total: self.total_bytes }
    }
}

struct BitrateColumn {
    bytes_interval: u64,
    total_bytes: u64,
}

impl BitrateColumn {
    fn new() -> Self {
        Self {
            bytes_interval: 0,
            total_bytes: 0,
        }
    }
}

impl Column for BitrateColumn {
    fn header(&self) -> &'static str {
        "Bitrate"
    }
    fn width(&self) -> usize {
        18
    }

    fn on_track(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }

    fn measure_interval(&self, interval: IntervalInfo) -> Metric {
        let bits = (self.bytes_interval as f64) * BITS_PER_BYTE;
        let mbps = if interval.elapsed_seconds > 0.0 {
            bits / (interval.elapsed_seconds * BITS_PER_MEGABIT)
        } else {
            0.0
        };
        Metric::Speed { mbps }
    }

    fn interval_reset(&mut self) {
        self.bytes_interval = 0;
    }

    fn measure_overall(&self, total_duration_s: f64) -> Metric {
        if total_duration_s <= 0.0 {
            return Metric::Speed { mbps: 0.0 };
        }

        let bits = (self.total_bytes as f64) * BITS_PER_BYTE;
        let mbps = bits / (total_duration_s * BITS_PER_MEGABIT);
        Metric::Speed { mbps }
    }
}

struct UdpLossColumn {
    tracker: UdpLossTracker,
}

impl UdpLossColumn {
    fn new() -> Self {
        Self {
            tracker: UdpLossTracker::new(),
        }
    }
}

impl Column for UdpLossColumn {
    fn header(&self) -> &'static str {
        "Loss"
    }
    fn width(&self) -> usize {
        18
    }

    fn on_track(&mut self, _bytes: usize, buf: &[u8]) {
        self.tracker.track(buf);
    }

    fn measure_interval(&self, _: IntervalInfo) -> Metric {
        let (rcv, exp, _lost, pct) = self.tracker.interval_loss();
        Metric::Loss {
            received: rcv,
            expected: exp,
            loss_pct: pct,
        }
    }

    fn interval_reset(&mut self) {
        self.tracker.interval_reset();
    }

    fn measure_overall(&self, _total_duration_s: f64) -> Metric {
        let (rcv, exp, _lost, pct) = self.tracker.overall_loss();
        Metric::Loss {
            received: rcv,
            expected: exp,
            loss_pct: pct,
        }
    }
}

struct UdpLossTracker {
    total_received: u64,
    highest_seq: u64,
    initialized: bool,
    prev_total_received: u64,
    prev_highest_seq: u64,
}

impl UdpLossTracker {
    fn new() -> Self {
        Self {
            total_received: 0,
            highest_seq: 0,
            initialized: false,
            prev_total_received: 0,
            prev_highest_seq: 0,
        }
    }

    fn track(&mut self, buf: &[u8]) {
        if buf.len() < 8 {
            return;
        }

        let seq_bytes: [u8; 8] = buf[..8].try_into().expect("Slice failed");
        let seq_num = u64::from_le_bytes(seq_bytes);
        self.track_packet(seq_num);
    }

    fn track_packet(&mut self, seq: u64) {
        self.total_received += 1;

        if !self.initialized {
            // first packet of this interval
            self.initialized = true;
            self.highest_seq = seq;

            self.prev_highest_seq = seq.saturating_sub(1);
        } else if seq > self.highest_seq {
            self.highest_seq = seq;
        }
    }

    fn interval_loss(&self) -> (u64, u64, u64, f64) {
        if !self.initialized {
            return (0, 0, 0, 0.0);
        }

        let delta_received = self.total_received - self.prev_total_received;
        let delta_expected = self.highest_seq - self.prev_highest_seq;

        (
            delta_received,
            delta_expected,
            // delta_expected might be greater than delta_received
            // if packets from the previous interval were delayed
            delta_expected.saturating_sub(delta_received),
            Self::calc_loss(delta_expected, delta_received),
        )
    }

    fn interval_reset(&mut self) {
        self.prev_total_received = self.total_received;
        self.prev_highest_seq = self.highest_seq;
    }

    fn overall_loss(&self) -> (u64, u64, u64, f64) {
        if !self.initialized {
            return (0, 0, 0, 0.0);
        }

        (
            self.total_received,
            self.highest_seq + 1,
            (self.highest_seq + 1).saturating_sub(self.total_received),
            Self::calc_loss(self.highest_seq + 1, self.total_received),
        )
    }

    fn calc_loss(expected: u64, received: u64) -> f64 {
        let lost = expected.saturating_sub(received);

        if expected == 0 { 0.0 } else { (lost as f64 * 100.0) / expected as f64 }
    }
}

struct UdpJitterColumn {
    tracker: UdpJitterTracker,
}

impl UdpJitterColumn {
    fn new() -> Self {
        Self {
            tracker: UdpJitterTracker::new(),
        }
    }
}

impl Column for UdpJitterColumn {
    fn header(&self) -> &'static str {
        "Jitter"
    }
    fn width(&self) -> usize {
        16
    }

    fn on_track(&mut self, _bytes: usize, buf: &[u8]) {
        self.tracker.track(buf, time::systime());
    }

    fn measure_interval(&self, _: IntervalInfo) -> Metric {
        Metric::Jitter {
            ms: self.tracker.get_jitter_ms(),
        }
    }

    fn interval_reset(&mut self) {
        self.tracker.interval_reset();
    }

    fn measure_overall(&self, _total_duration_s: f64) -> Metric {
        Metric::Jitter {
            ms: self.tracker.get_jitter_ms(),
        }
    }
}

struct UdpJitterTracker {
    jitter_ms: f64,
    prev_transit: Option<TimeDelta>,
}

impl UdpJitterTracker {
    fn new() -> Self {
        Self {
            jitter_ms: 0.0,
            prev_transit: None,
        }
    }

    fn track(&mut self, buf: &[u8], recv_time: TimeDelta) {
        if buf.len() < 16 {
            return;
        }

        // parse send time
        let ts_bytes: [u8; 8] = buf[8..16].try_into().expect("Slice failed");
        let send_time_secs = f64::from_le_bytes(ts_bytes);
        let send_time = TimeDelta::microseconds((send_time_secs * 1_000_000.0) as i64);

        self.update_jitter(send_time, recv_time);
    }

    fn update_jitter(&mut self, send_time: TimeDelta, recv_time: TimeDelta) {
        let transit_time = recv_time - send_time;

        if let Some(prev) = self.prev_transit {
            // delta is the difference between current transit and previous transit
            let delta = (transit_time - prev).abs();

            let delta_ms = delta.num_microseconds().unwrap_or(0) as f64 / 1000.0;

            // apply smoothing factor 1/16
            self.jitter_ms += (delta_ms - self.jitter_ms) / 16.0;
        }

        self.prev_transit = Some(transit_time);
    }

    fn interval_reset(&self) {
        // do nothing
    }

    fn get_jitter_ms(&self) -> f64 {
        self.jitter_ms
    }
}

pub struct Stats {
    interval: Mutex<ReportInterval>,
    trackers: Mutex<BTreeMap<usize, StatsTracker>>,
    sum_tracker: Mutex<Option<StatsTracker>>,
    protocol: Protocol,
    role: Role,
}

impl Stats {
    pub fn tcp(interval_seconds: u32, total_time_seconds: u32) -> Self {
        Self {
            interval: Mutex::new(ReportInterval::new(interval_seconds, total_time_seconds)),
            trackers: Mutex::new(BTreeMap::new()),
            sum_tracker: Mutex::new(None),
            protocol: Protocol::Tcp,
            role: Role::Sender,
        }
    }

    pub fn udp(interval_seconds: u32, total_time_seconds: u32, role: Role) -> Self {
        Self {
            interval: Mutex::new(ReportInterval::new(interval_seconds, total_time_seconds)),
            trackers: Mutex::new(BTreeMap::new()),
            sum_tracker: Mutex::new(None),
            protocol: Protocol::Udp,
            role,
        }
    }

    fn create_tracker(&self, thread_id: usize) -> StatsTracker {
        let mut columns: Vec<Box<dyn Column + Send>> = vec![
            Box::new(IdColumn::new(thread_id)),
            Box::new(IntervalColumn::new()),
            Box::new(TransferColumn::new()),
            Box::new(BitrateColumn::new()),
        ];

        if let (Protocol::Udp, Role::Receiver) = (self.protocol, self.role) {
            columns.push(Box::new(UdpLossColumn::new()));
            columns.push(Box::new(UdpJitterColumn::new()));
        }

        StatsTracker::new(columns)
    }

    pub fn register_thread(&self, thread_id: usize) {
        let mut trackers = self.trackers.lock();

        if !trackers.contains_key(&thread_id) {
            trackers.insert(thread_id, self.create_tracker(thread_id));
        }

        let mut sum_tracker_guard = self.sum_tracker.lock();

        if sum_tracker_guard.is_none() && trackers.len() > 1 {
            *sum_tracker_guard = Some(self.create_tracker(usize::MAX));
        }
    }

    pub fn has_total_time_elapsed(&self) -> bool {
        self.interval.lock().has_total_time_elapsed()
    }

    pub fn get_header(&self) -> String {
        let trackers = self.trackers.lock();

        if let Some((_, tracker)) = trackers.iter().next() {
            tracker.get_header()
        } else {
            self.create_tracker(0).get_header()
        }
    }

    pub fn track(&self, thread_id: usize, bytes: usize, buf: &[u8]) {
        let mut trackers = self.trackers.lock();
        if let Some(tracker) = trackers.get_mut(&thread_id) {
            tracker.track(bytes, buf);
        }
        drop(trackers);

        let mut sum_tracker = self.sum_tracker.lock();
        if let Some(tracker) = sum_tracker.as_mut() {
            tracker.track(bytes, buf);
        }
    }

    pub fn print_interval_info(&self) {
        let mut interval = self.interval.lock();

        if let Some(info) = interval.check() {
            drop(interval);
            let mut trackers = self.trackers.lock();
            let mut sum_tracker = self.sum_tracker.lock();

            if let Some(tracker) = sum_tracker.as_mut() {
                println!("{}", tracker.get_header());
            }

            for (_, tracker) in trackers.iter_mut() {
                tracker.print_interval_info(info);
            }
            drop(trackers);

            if let Some(tracker) = sum_tracker.as_mut() {
                tracker.print_interval_info(info);
                println!("- - - - - - - - - - - - - - - - - - - - - - -");
            }
        }
    }

    pub fn finalize_and_get_summary(&self) -> String {
        let mut interval = self.interval.lock();

        if let Some(info) = interval.finalize_pending_interval() {
            drop(interval);
            let mut trackers = self.trackers.lock();
            let mut sum_tracker = self.sum_tracker.lock();

            if let Some(tracker) = sum_tracker.as_mut() {
                println!("{}", tracker.get_header());
            }

            for (_, tracker) in trackers.iter_mut() {
                tracker.print_interval_info(info);
            }
            drop(trackers);

            if let Some(tracker) = sum_tracker.as_mut() {
                tracker.print_interval_info(info);
            }
        }

        let interval = self.interval.lock();
        let total_duration = interval.total_duration_s();
        drop(interval);

        let mut trackers = self.trackers.lock();
        self.build_summary(&mut trackers, total_duration)
    }

    fn build_summary(&self, trackers: &mut BTreeMap<usize, StatsTracker>, total_duration_s: f64) -> String {
        let mut lines = Vec::new();

        for (_, tracker) in trackers.iter_mut() {
            lines.push(tracker.build_summary(total_duration_s));
        }

        let mut sum_tracker = self.sum_tracker.lock();
        if let Some(tracker) = sum_tracker.as_mut() {
            lines.push(tracker.build_summary(total_duration_s));
        }

        lines.join("\n")
    }

    pub fn stats_as_json(&self) -> String {
        let interval = self.interval.lock();
        let total_duration = interval.total_duration_s();
        drop(interval);

        let trackers = self.trackers.lock();
        let mut stream_jsons = Vec::new();

        for (thread_id, tracker) in trackers.iter() {
            stream_jsons.push(alloc::format!(
                "{{ \"stream_id\": {}, {} }}",
                thread_id,
                &tracker.get_json(total_duration)[1..tracker.get_json(total_duration).len() - 1]
            ));
        }

        alloc::format!("{{ \"streams\": [{}] }}", stream_jsons.join(", "))
    }
}
