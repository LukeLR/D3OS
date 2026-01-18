extern crate alloc;
use core::fmt::Write;

use crate::cli::Protocol;
use crate::Role;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::{format, vec};
use chrono::TimeDelta;
use core::cmp::min;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;
use terminal::println;

const BITS_PER_BYTE: f64 = 8.0;
const BYTES_PER_MEGABYTE: f64 = 1024.0 * 1024.0;
const BITS_PER_MEGABIT: f64 = 1_000_000.0;

const COL_SEP: &str = " ";

/// Trait for a column in the statistics output.
trait Column {
    fn header(&self) -> &'static str;
    fn width(&self) -> usize;
    /// Update the column with new packet data.
    fn update(&mut self, _bytes: usize, _buf: &[u8]) {}
    /// Calculate metric for the current interval and reset internal counters.
    fn measure_interval(&mut self, interval: IntervalInfo) -> Metric;
    /// Calculate metric for the total duration.
    fn measure_overall(&self, total_duration_s: f64) -> Metric;
}

#[derive(Clone)]
enum Metric {
    Bytes { total: u64 },
    Speed { mbps: f64 },
    Loss { received: u64, expected: u64, loss_pct: f64 },
    Jitter { ms: f64 },
    Id { id: usize },
    Interval { start_s: f64, end_s: f64 },
}

impl Metric {
    /// Merges another metric into this one
    fn merge(&mut self, other: &Metric) {
        match (self, other) {
            (Metric::Bytes { total: t1 }, Metric::Bytes { total: t2 }) => *t1 += t2,
            (Metric::Speed { mbps: m1 }, Metric::Speed { mbps: m2 }) => *m1 += m2,
            (
                Metric::Loss {
                    received: r1,
                    expected: e1,
                    loss_pct,
                },
                Metric::Loss {
                    received: r2, expected: e2, ..
                },
            ) => {
                *r1 += r2;
                *e1 += e2;
                // Recalculate percentage based on new totals
                *loss_pct = UdpLossTracker::calc_loss(*e1, *r1);
            }
            (Metric::Jitter { ms: m1 }, Metric::Jitter { ms: m2 }) => {
                // Average the jitter
                *m1 = (*m1 + *m2) / 2.0;
            }
            _ => {}
        }
    }

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

struct ReportRow {
    formatted_line: String,
    raw_metrics: Vec<Metric>,
}

struct StatsTracker {
    columns: Vec<Box<dyn Column + Send>>,
    interval_json_data: Vec<String>,
    final_json_data: Option<String>,
}

impl StatsTracker {
    fn new(columns: Vec<Box<dyn Column + Send>>) -> Self {
        Self {
            columns,
            interval_json_data: Vec::new(),
            final_json_data: None,
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

    fn update(&mut self, bytes: usize, buf: &[u8]) {
        for c in &mut self.columns {
            c.update(bytes, buf);
        }
    }

    fn build_report(&mut self, info: IntervalInfo) -> ReportRow {
        let mut metrics = Vec::with_capacity(self.columns.len());
        let mut line = String::new();
        let mut json = String::new();
        let _ = write!(json, "{{ \"seconds\": {:.2}", info.elapsed_seconds);

        for (i, column) in self.columns.iter_mut().enumerate() {
            if i > 0 {
                line.push_str(COL_SEP);
            }

            let measurement = column.measure_interval(info);
            let _ = write!(line, "{:>width$}", measurement.to_console_string(), width = column.width());
            let _ = write!(json, ", {}", measurement.to_json());
            metrics.push(measurement);
        }

        json.push_str(" }");
        self.interval_json_data.push(json);

        ReportRow {
            formatted_line: line,
            raw_metrics: metrics,
        }
    }

    fn build_summary(&mut self, total_duration_s: f64) -> ReportRow {
        let mut metrics = Vec::with_capacity(self.columns.len());
        let mut line = String::new();
        let mut json = String::new();
        let _ = write!(json, "\"summary\": {{ \"duration_seconds\": {:.4}", total_duration_s);

        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                line.push_str(COL_SEP);
            }

            let measurement = c.measure_overall(total_duration_s);
            let _ = write!(line, "{:>width$}", measurement.to_console_string(), width = c.width());
            let _ = write!(json, ", {}", measurement.to_json());
            metrics.push(measurement);
        }

        json.push_str(" }, \"intervals\": [");

        for (i, interval) in self.interval_json_data.iter().enumerate() {
            if i > 0 {
                json.push_str(", ");
            }

            json.push_str(interval);
        }

        json.push_str("]");
        self.final_json_data = Some(json);

        ReportRow {
            formatted_line: line,
            raw_metrics: metrics,
        }
    }

    fn get_json(&mut self) -> String {
        self.final_json_data.take().expect("StatsTracker does not have any json data")
    }
}

#[derive(Copy, Clone)]
struct IntervalInfo {
    elapsed_seconds: f64,
    interval_start_s: f64,
    interval_end_s: f64,
}

struct ReportInterval {
    start_time: TimeDelta,
    report_interval: TimeDelta,
    total_duration: TimeDelta,
    last_report_time: TimeDelta,
    stop_on_timer: bool,
}

impl ReportInterval {
    fn new(interval_seconds: u32, total_time_seconds: u32, stop_on_timer: bool) -> Self {
        let now = time::systime();
        Self {
            start_time: now,
            report_interval: TimeDelta::seconds(interval_seconds as i64),
            total_duration: TimeDelta::seconds(total_time_seconds as i64),
            last_report_time: now,
            stop_on_timer,
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

    fn finalize_pending_interval(&mut self) -> Option<IntervalInfo> {
        let expected_end_time = self.start_time + self.total_duration;
        let mut interval_end_time = time::systime();

        if self.stop_on_timer {
            interval_end_time = min(interval_end_time, expected_end_time);
        }

        if interval_end_time <= self.last_report_time {
            return None;
        }

        let elapsed = interval_end_time - self.last_report_time;

        let info = IntervalInfo {
            elapsed_seconds: elapsed.as_seconds_f64(),
            interval_start_s: (self.last_report_time - self.start_time).as_seconds_f64(),
            interval_end_s: (interval_end_time - self.start_time).as_seconds_f64(),
        };

        self.last_report_time = interval_end_time;
        Some(info)
    }

    fn has_time_elapsed(&self) -> bool {
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

    fn measure_interval(&mut self, interval: IntervalInfo) -> Metric {
        Metric::Interval {
            start_s: interval.interval_start_s,
            end_s: interval.interval_end_s,
        }
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

    fn measure_interval(&mut self, _: IntervalInfo) -> Metric {
        Metric::Id { id: self.id }
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

    fn update(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }

    fn measure_interval(&mut self, _: IntervalInfo) -> Metric {
        let m = Metric::Bytes { total: self.bytes_interval };
        self.bytes_interval = 0;
        m
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
    fn update(&mut self, bytes: usize, _buf: &[u8]) {
        let b = bytes as u64;
        self.bytes_interval += b;
        self.total_bytes += b;
    }
    fn measure_interval(&mut self, interval: IntervalInfo) -> Metric {
        let bits = (self.bytes_interval as f64) * BITS_PER_BYTE;
        let mbps = if interval.elapsed_seconds > 0.0 {
            bits / (interval.elapsed_seconds * BITS_PER_MEGABIT)
        } else {
            0.0
        };
        self.bytes_interval = 0;
        Metric::Speed { mbps }
    }
    fn measure_overall(&self, total_duration_s: f64) -> Metric {
        let bits = (self.total_bytes as f64) * BITS_PER_BYTE;
        let mbps = if total_duration_s > 0.0 {
            bits / (total_duration_s * BITS_PER_MEGABIT)
        } else {
            0.0
        };
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
        20
    }

    fn update(&mut self, _bytes: usize, buf: &[u8]) {
        self.tracker.track(buf);
    }

    fn measure_interval(&mut self, _: IntervalInfo) -> Metric {
        let (rcv, exp, _lost, pct) = self.tracker.interval_loss();
        self.tracker.interval_reset();
        Metric::Loss {
            received: rcv,
            expected: exp,
            loss_pct: pct,
        }
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

        let exp = self.highest_seq + 1;

        (
            self.total_received,
            exp,
            exp.saturating_sub(self.total_received),
            Self::calc_loss(exp, self.total_received),
        )
    }
    fn calc_loss(exp: u64, rcv: u64) -> f64 {
        if exp == 0 {
            0.0
        } else {
            (exp.saturating_sub(rcv) as f64 * 100.0) / exp as f64
        }
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
        12
    }

    fn update(&mut self, _bytes: usize, buf: &[u8]) {
        self.tracker.track(buf, time::systime());
    }

    fn measure_interval(&mut self, _: IntervalInfo) -> Metric {
        Metric::Jitter {
            ms: self.tracker.get_jitter_ms(),
        }
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

    fn get_jitter_ms(&self) -> f64 {
        self.jitter_ms
    }
}

/// Handle returned to the worker thread to record stats without map lookups.
pub struct StreamStats {
    tracker: Arc<Mutex<StatsTracker>>,
    global_bytes: Arc<AtomicU64>,
}

impl StreamStats {
    pub fn track(&self, bytes: usize, buf: &[u8]) {
        self.global_bytes.fetch_add(bytes as u64, Ordering::Relaxed);

        let mut t = self.tracker.lock();
        t.update(bytes, buf);
    }
}

pub struct Stats {
    interval: Mutex<ReportInterval>,
    trackers: Mutex<BTreeMap<usize, Arc<Mutex<StatsTracker>>>>,
    global_transferred: Arc<AtomicU64>,
    limit_bytes: Option<u64>,
    protocol: Protocol,
    role: Role,
}

impl Stats {
    pub fn new(protocol: Protocol, role: Role, interval_seconds: u32, total_time_seconds: u32, limit_bytes: Option<u64>) -> Self {
        Self {
            interval: Mutex::new(ReportInterval::new(interval_seconds, total_time_seconds, limit_bytes.is_some())),
            trackers: Mutex::new(BTreeMap::new()),
            global_transferred: Arc::new(AtomicU64::new(0)),
            limit_bytes,
            protocol,
            role,
        }
    }

    pub fn tcp(interval_seconds: u32, total_time_seconds: u32, limit_bytes: Option<u64>) -> Self {
        Self::new(Protocol::Tcp, Role::Sender, interval_seconds, total_time_seconds, limit_bytes)
    }

    pub fn udp(interval_seconds: u32, total_time_seconds: u32, role: Role, limit_bytes: Option<u64>) -> Self {
        Self::new(Protocol::Udp, role, interval_seconds, total_time_seconds, limit_bytes)
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

    /// Registers a thread and returns a handle to track its stats
    pub fn register_thread(&self, thread_id: usize) -> StreamStats {
        let mut trackers = self.trackers.lock();

        let tracker = trackers
            .entry(thread_id)
            .or_insert_with(|| Arc::new(Mutex::new(self.create_tracker(thread_id))))
            .clone();

        StreamStats {
            tracker,
            global_bytes: self.global_transferred.clone(),
        }
    }

    /// Checks if the benchmark is finished based on transferred bytes if used, otherwise time.
    pub fn is_finished(&self) -> bool {
        if let Some(limit) = self.limit_bytes {
            return self.global_transferred.load(Ordering::Relaxed) >= limit;
        }

        self.interval.lock().has_time_elapsed()
    }

    pub fn get_header(&self) -> String {
        let trackers = self.trackers.lock();

        if let Some((_, tracker)) = trackers.iter().next() {
            tracker.lock().get_header()
        } else {
            self.create_tracker(0).get_header()
        }
    }

    fn calculate_sum_row(&self, rows: &[ReportRow]) -> Option<String> {
        if rows.len() <= 1 {
            return None;
        }

        let mut sum = rows[0].raw_metrics.clone();

        // Set ID to SUM
        for m in &mut sum {
            if let Metric::Id { id } = m {
                *id = usize::MAX;
            }
        }

        // Merge the rows
        for row in &rows[1..] {
            for (i, m) in row.raw_metrics.iter().enumerate() {
                if i < sum.len() {
                    sum[i].merge(m);
                }
            }
        }

        let trackers = self.trackers.lock();
        if let Some((_, tracker)) = trackers.iter().next() {
            let tracker = tracker.lock();
            let mut line = String::new();

            for (i, (col, m)) in tracker.columns.iter().zip(sum).enumerate() {
                if i > 0 {
                    line.push_str(COL_SEP);
                }
                let _ = write!(line, "{:>width$}", m.to_console_string(), width = col.width());
            }

            return Some(line);
        }

        None
    }

    fn collect_output<F>(&self, mut get_row: F) -> (Vec<String>, Option<String>)
    where
        F: FnMut(&mut StatsTracker) -> ReportRow,
    {
        let trackers = self.trackers.lock();
        let mut rows = Vec::new();

        for (_, tracker) in trackers.iter() {
            let mut t = tracker.lock();
            rows.push(get_row(&mut t));
        }
        drop(trackers);

        let lines = rows.iter().map(|r| r.formatted_line.clone()).collect();
        let sum_line = self.calculate_sum_row(&rows);

        (lines, sum_line)
    }

    fn print_on_interval(&self, info: Option<IntervalInfo>) {
        if let Some(info) = info {
            let (lines, sum) = self.collect_output(|t| t.build_report(info));

            if sum.is_some() {
                println!("{}", self.get_header());
            }

            for line in lines {
                println!("{}", line);
            }

            if let Some(s) = sum {
                println!("{}", s);
                println!("- - - - - - - - - - - - - - - - - - - - - - -");
            }
        }
    }

    pub fn print_interval_report(&self) {
        let mut interval = self.interval.lock();

        self.print_on_interval(interval.check());
    }

    pub fn finalize_and_get_summary(&self) -> String {
        let mut interval = self.interval.lock();
        let interval_info = interval.finalize_pending_interval();
        let total_duration = interval_info.map_or(interval.total_duration_s(), |i| i.interval_end_s);
        drop(interval);

        self.print_on_interval(interval_info);
        let (mut lines, sum) = self.collect_output(|t| t.build_summary(total_duration));

        if let Some(s) = sum {
            lines.push(s);
        }

        format!("{}\n{}", self.get_header(), lines.join("\n"))
    }

    pub fn as_json(&self) -> String {
        let trackers = self.trackers.lock();
        let mut stream_jsons = Vec::new();

        for (thread_id, tracker) in trackers.iter() {
            stream_jsons.push(format!("{{ \"stream_id\": {}, {} }}", thread_id, &tracker.lock().get_json()));
        }

        format!("{{ \"streams\": [{}] }}", stream_jsons.join(", "))
    }
}
