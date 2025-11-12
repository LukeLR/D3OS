use alloc::string::String;
use chrono::TimeDelta;
use terminal::println;
#[allow(unused_imports)]
use runtime::*;

const BITS_PER_BYTE: f64 = 8.0;
const BYTES_PER_MEGABYTE: f64 = 1024.0 * 1024.0;
const BITS_PER_MEGABIT: f64 = 1_000_000.0;

pub struct StatsTracker {
    last_report_time: TimeDelta,
    start_time: TimeDelta,
    bytes_this_interval: u64,
    report_interval: TimeDelta,
}

impl StatsTracker {
    pub fn new(interval_seconds: u32) -> Self {
        let now = time::systime();
        Self {
            last_report_time: now,
            start_time: now,
            bytes_this_interval: 0,
            report_interval: TimeDelta::seconds(interval_seconds as i64),
        }
    }

    pub fn track(&mut self, bytes: usize) {
        self.bytes_this_interval += bytes as u64;
    }

    pub fn check_and_print_interval_report(&mut self) {
        let current_time = time::systime();
        let elapsed = current_time - self.last_report_time;

        if elapsed >= self.report_interval {
            let elapsed_seconds = elapsed.as_seconds_f64();

            if elapsed_seconds > 0.0 {
                let bytes_f64 = self.bytes_this_interval as f64;

                let megabits_per_second = (bytes_f64 * BITS_PER_BYTE) / (elapsed_seconds * BITS_PER_MEGABIT);
                let transfer_mb = bytes_f64 / BYTES_PER_MEGABYTE;

                let interval_start_s = (self.last_report_time - self.start_time).as_seconds_f64();
                let interval_end_s = (current_time - self.start_time).as_seconds_f64();

                println!(
                    "[  1] {:>4.2}-{:>4.2} sec   {:>6.2} MBytes   {:>6.2} Mbits/sec",
                    interval_start_s, interval_end_s, transfer_mb, megabits_per_second
                );
            }

            self.last_report_time = current_time;
            self.bytes_this_interval = 0;
        }
    }

    pub fn has_total_time_elapsed(&self) -> bool {
        time::systime() - self.start_time >= TimeDelta::seconds(10)
    }
    
    pub fn finalize_and_get_summary(&self) -> String {
        String::from("to be implemented...")
    }

    pub fn get_header(&self) -> String {
        String::from("[ ID] Interval           Transfer     Bitrate")
    }
}