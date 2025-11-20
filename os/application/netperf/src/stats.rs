extern crate alloc;
use crate::Role;
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use chrono::TimeDelta;
#[allow(unused_imports)]
use runtime::*;
use terminal::println;

const BITS_PER_BYTE: f64 = 8.0;
const BYTES_PER_MEGABYTE: f64 = 1024.0 * 1024.0;
const BITS_PER_MEGABIT: f64 = 1_000_000.0;

const INTERVAL_COL_WIDTH: usize = 16;
const COL_SEP: &str = " ";

trait Column {
    fn header(&self) -> &'static str;
    fn width(&self) -> usize;
    fn on_track(&mut self, _bytes: usize, _buf: &[u8]) {}
    fn interval_cell_and_reset(&mut self, elapsed_s: f64) -> String;
    fn overall_cell(&self, total_duration_s: f64) -> String;
}

struct GenericStatsTracker {
    columns: Vec<Box<dyn Column>>,
}

impl GenericStatsTracker {
    fn new(columns: Vec<Box<dyn Column>>) -> Self {
        Self { columns }
    }

    fn get_header(&self) -> String {
        // Interval header cell
        let mut line = alloc::format!("{:>width$}", "Interval", width = INTERVAL_COL_WIDTH);
        // Column headers
        for c in &self.columns {
            line.push_str(COL_SEP);
            line.push_str(&alloc::format!("{:>width$}", c.header(), width = c.width()));
        }
        line
    }

    fn track(&mut self, bytes: usize, buf: &[u8]) {
        for c in &mut self.columns {
            c.on_track(bytes, buf);
        }
    }

    fn build_interval_string(&mut self, info: IntervalInfo) -> Option<String> {
        if info.elapsed_seconds <= 0.0 {
            return None;
        }
        let prefix = alloc::format!("{:>4.2}-{:>4.2} sec", info.interval_start_s, info.interval_end_s);
        let mut line = alloc::format!("{:>width$}", prefix, width = INTERVAL_COL_WIDTH);
        for c in &mut self.columns {
            let cell = c.interval_cell_and_reset(info.elapsed_seconds);
            line.push_str(COL_SEP);
            line.push_str(&alloc::format!("{:>width$}", cell, width = c.width()));
        }
        Some(line)
    }

    fn print_interval_info(&mut self, info: IntervalInfo) {
        if let Some(s) = self.build_interval_string(info) {
            println!("{}", s);
        }
    }

    fn build_summary(&self, total_duration_s: f64) -> Option<String> {
        if total_duration_s <= 0.0 {
            return None;
        }
        let prefix = alloc::format!("{:>4.2}-{:>4.2} sec", 0.0, total_duration_s);
        let mut line = alloc::format!("{:>width$}", prefix, width = INTERVAL_COL_WIDTH);
        for c in &self.columns {
            let cell = c.overall_cell(total_duration_s);
            line.push_str(COL_SEP);
            line.push_str(&alloc::format!("{:>width$}", cell, width = c.width()));
        }
        Some(line)
    }
}

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
        20
    }

    fn on_track(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }

    fn interval_cell_and_reset(&mut self, _elapsed_s: f64) -> String {
        let mb = (self.bytes_interval as f64) / BYTES_PER_MEGABYTE;
        self.bytes_interval = 0;
        alloc::format!("{:>6.2} MBytes", mb)
    }

    fn overall_cell(&self, _total_duration_s: f64) -> String {
        let total_mb = (self.total_bytes as f64) / BYTES_PER_MEGABYTE;
        alloc::format!("{:>6.2} MBytes", total_mb)
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
        20
    }

    fn on_track(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }

    fn interval_cell_and_reset(&mut self, elapsed_s: f64) -> String {
        let bits = (self.bytes_interval as f64) * BITS_PER_BYTE;
        let mbps = if elapsed_s > 0.0 { bits / (elapsed_s * BITS_PER_MEGABIT) } else { 0.0 };
        self.bytes_interval = 0;
        alloc::format!("{:>6.2} Mbits/sec", mbps)
    }

    fn overall_cell(&self, total_duration_s: f64) -> String {
        if total_duration_s <= 0.0 {
            return alloc::format!("{:>6.2} Mbits/sec", 0.0);
        }
        let bits = (self.total_bytes as f64) * BITS_PER_BYTE;
        let avg = bits / (total_duration_s * BITS_PER_MEGABIT);
        alloc::format!("{:>6.2} Mbits/sec", avg)
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

    fn interval_cell_and_reset(&mut self, _elapsed_s: f64) -> String {
        let (_rcv, exp, lost, pct) = self.tracker.interval_loss();
        alloc::format!("{}/{} ({:.2}%)", lost, exp, pct)
    }

    fn overall_cell(&self, _total_duration_s: f64) -> String {
        let (_rcv, exp, lost, pct) = self.tracker.overall_loss();
        alloc::format!("{}/{} ({:.2}%)", lost, exp, pct)
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

    fn interval_loss(&mut self) -> (u64, u64, u64, f64) {
        if !self.initialized {
            return (0, 0, 0, 0.0);
        }

        let delta_received = self.total_received - self.prev_total_received;
        let delta_expected = self.highest_seq - self.prev_highest_seq;

        self.prev_total_received = self.total_received;
        self.prev_highest_seq = self.highest_seq;

        (
            delta_received,
            delta_expected,
            // delta_expected might be greater than delta_received
            // if packets from the previous interval were delayed
            delta_expected.saturating_sub(delta_received),
            Self::calc_loss(delta_expected, delta_received),
        )
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

    fn interval_cell_and_reset(&mut self, _elapsed_s: f64) -> String {
        alloc::format!("{:.3} ms", self.tracker.get_jitter_ms())
    }

    fn overall_cell(&self, _total_duration_s: f64) -> String {
        alloc::format!("{:.3} ms", self.tracker.get_jitter_ms())
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

    fn get_jitter_ms(&self) -> f64 {
        self.jitter_ms
    }
}

pub struct Stats {
    interval: ReportInterval,
    core: GenericStatsTracker,
}

impl Stats {
    pub fn tcp(interval_seconds: u32, total_time_seconds: u32) -> Self {
        let columns: Vec<Box<dyn Column>> = vec![Box::new(TransferColumn::new()), Box::new(BitrateColumn::new())];
        Self {
            interval: ReportInterval::new(interval_seconds, total_time_seconds),
            core: GenericStatsTracker::new(columns),
        }
    }

    pub fn udp(interval_seconds: u32, total_time_seconds: u32, role: Role) -> Self {
        let mut columns: Vec<Box<dyn Column>> = vec![Box::new(TransferColumn::new()), Box::new(BitrateColumn::new())];

        if let Role::Receiver = role {
            columns.push(Box::new(UdpLossColumn::new()));
            columns.push(Box::new(UdpJitterColumn::new()));
        }

        Self {
            interval: ReportInterval::new(interval_seconds, total_time_seconds),
            core: GenericStatsTracker::new(columns),
        }
    }

    pub fn has_total_time_elapsed(&self) -> bool {
        self.interval.has_total_time_elapsed()
    }

    pub fn get_header(&self) -> String {
        self.core.get_header()
    }

    pub fn track(&mut self, bytes: usize, buf: &[u8]) {
        self.core.track(bytes, buf);
    }

    pub fn print_interval_info(&mut self) {
        if let Some(info) = self.interval.check() {
            self.core.print_interval_info(info);
        }
    }

    pub fn finalize_and_get_summary(&mut self) -> String {
        if let Some(info) = self.interval.finalize_pending_interval() {
            self.core.print_interval_info(info);
        }
        let total = self.interval.total_duration_s();
        if let Some(s) = self.core.build_summary(total) { s } else { String::from("") }
    }
}
