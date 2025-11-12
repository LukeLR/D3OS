extern crate alloc;
use alloc::{boxed::Box, string::String, vec, vec::Vec};
use chrono::TimeDelta;
#[allow(unused_imports)]
use runtime::*;
use terminal::{println};
use crate::Role;

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
        if total_duration_s <= 0.0 { return None; }
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

pub struct IntervalInfo {
    pub elapsed_seconds: f64,
    pub interval_start_s: f64,
    pub interval_end_s: f64,
}

pub struct ReportInterval {
    last_report_time: TimeDelta,
    start_time: TimeDelta,
    report_interval: TimeDelta,
    total_duration: TimeDelta,
}

impl ReportInterval {
    pub fn new(interval_seconds: u32, total_time_seconds: u32) -> Self {
        let now = time::systime();
        Self {
            last_report_time: now,
            start_time: now,
            report_interval: TimeDelta::seconds(interval_seconds as i64),
            total_duration: TimeDelta::seconds(total_time_seconds as i64),
        }
    }

    pub fn check(&mut self) -> Option<IntervalInfo> {
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

    pub fn total_duration_s(&self) -> f64 {
        self.total_duration.as_seconds_f64()
    }

    fn clamp_now_to_end(&self) -> TimeDelta {
        let now = time::systime();
        let end = self.start_time + self.total_duration;
        if now > end { end } else { now }
    }

    /// Finalize any pending partial interval and advance the last_report_time.
    pub fn finalize_pending_interval(&mut self) -> Option<IntervalInfo> {
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

    pub fn has_total_time_elapsed(&self) -> bool {
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
    fn header(&self) -> &'static str { "Transfer" }
    fn width(&self) -> usize { 20 }

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
    fn header(&self) -> &'static str { "Bitrate" }
    fn width(&self) -> usize { 20 }

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
    fn header(&self) -> &'static str { "Loss" }
    fn width(&self) -> usize { 18 }

    fn on_track(&mut self, _bytes: usize, buf: &[u8]) {
        self.tracker.track(buf);
    }

    fn interval_cell_and_reset(&mut self, _elapsed_s: f64) -> String {
        let (_rcv, exp, lost, pct) = self.tracker.take_interval_loss();
        alloc::format!("{}/{} ({:.2}%)", lost, exp, pct)
    }

    fn overall_cell(&self, _total_duration_s: f64) -> String {
        let (_rcv, exp, lost, pct) = self.tracker.overall_loss();
        alloc::format!("{}/{} ({:.2}%)", lost, exp, pct)
    }
}

pub struct UdpLossTracker {
    // Cumulative
    total_received_packets: u64,
    highest_seq_received: u64,
    first_packet: bool,
    // Per-interval
    interval_received_packets: u64,
    interval_min_seq: u64,
    interval_max_seq: u64,
    interval_saw_any: bool,
}

impl UdpLossTracker {
    pub fn new() -> Self {
        Self {
            total_received_packets: 0,
            highest_seq_received: 0,
            first_packet: true,
            interval_received_packets: 0,
            interval_min_seq: 0,
            interval_max_seq: 0,
            interval_saw_any: false,
        }
    }

    pub fn track(&mut self, buf: &[u8]) {
        if buf.len() >= 8 {
            let seq_bytes: [u8; 8] = buf[..8].try_into().expect("Slice failed");
            let seq_num = u64::from_le_bytes(seq_bytes);
            self.record_packet(seq_num);
        }
    }

    fn record_packet(&mut self, seq_num: u64) {
        self.total_received_packets += 1;

        if self.first_packet || seq_num > self.highest_seq_received {
            self.highest_seq_received = seq_num;
            self.first_packet = false;
        }

        // interval stats
        if !self.interval_saw_any {
            self.interval_min_seq = seq_num;
            self.interval_max_seq = seq_num;
            self.interval_received_packets = 1;
            self.interval_saw_any = true;
        } else {
            if seq_num < self.interval_min_seq {
                self.interval_min_seq = seq_num;
            }
            if seq_num > self.interval_max_seq {
                self.interval_max_seq = seq_num;
            }
            self.interval_received_packets += 1;
        }
    }

    pub fn take_interval_loss(&mut self) -> (u64, u64, u64, f64) {
        if !self.interval_saw_any {
            return (0, 0, 0, 0.0);
        }
        let expected = (self.interval_max_seq - self.interval_min_seq + 1) as u64;
        let received = self.interval_received_packets;
        let lost = expected.saturating_sub(received);
        let pct = if expected == 0 { 0.0 } else { (lost as f64) * 100.0 / expected as f64 };
        // reset interval
        self.interval_received_packets = 0;
        self.interval_min_seq = 0;
        self.interval_max_seq = 0;
        self.interval_saw_any = false;
        (received, expected, lost, pct)
    }

    pub fn overall_loss(&self) -> (u64, u64, u64, f64) {
        let total_expected = if self.first_packet { 0 } else { self.highest_seq_received + 1 };
        let received = self.total_received_packets;
        let lost_packets = total_expected.saturating_sub(received);
        let loss_percent = if total_expected == 0 {
            0.0
        } else {
            (lost_packets as f64 * 100.0) / total_expected as f64
        };
        (received, total_expected, lost_packets, loss_percent)
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
        if let Some(s) = self.core.build_summary(total) {
            s
        } else {
            String::from("")
        }
    }
}
