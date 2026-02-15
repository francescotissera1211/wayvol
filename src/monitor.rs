//! Monitor thread that watches for PipeWire stream changes.
//!
//! Uses `pw-dump --monitor` to get real-time JSON events when streams
//! appear/disappear. Falls back to polling `wpctl status` if pw-dump
//! is unavailable.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use async_channel::Sender;

/// Events sent from the monitor thread to the UI.
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// Streams have changed — UI should refresh.
    StreamsChanged,
    /// Monitor encountered an error.
    Error(String),
}

/// Handle to the running monitor thread.
pub struct MonitorThread {
    handle: Option<JoinHandle<()>>,
    child: Option<Child>,
}

impl MonitorThread {
    /// Spawn the monitor thread. It will send `StreamsChanged` events
    /// whenever PipeWire state changes.
    pub fn spawn(event_tx: Sender<MonitorEvent>) -> Result<Self> {
        // Try pw-dump --monitor first for real-time events
        let child_result = Command::new("pw-dump")
            .arg("--monitor")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        match child_result {
            Ok(mut child) => {
                let stdout = child
                    .stdout
                    .take()
                    .expect("pw-dump stdout should be piped");

                let handle = thread::Builder::new()
                    .name("pw-monitor".into())
                    .spawn(move || {
                        Self::run_pw_dump_monitor(stdout, event_tx);
                    })?;

                Ok(Self {
                    handle: Some(handle),
                    child: Some(child),
                })
            }
            Err(e) => {
                log::warn!("pw-dump not available ({e}), falling back to polling");

                let handle = thread::Builder::new()
                    .name("pw-monitor-poll".into())
                    .spawn(move || {
                        Self::run_polling_monitor(event_tx);
                    })?;

                Ok(Self {
                    handle: Some(handle),
                    child: None,
                })
            }
        }
    }

    /// Monitor using pw-dump --monitor output.
    /// Each time pw-dump outputs a JSON array, streams may have changed.
    fn run_pw_dump_monitor(
        stdout: std::process::ChildStdout,
        event_tx: Sender<MonitorEvent>,
    ) {
        let reader = BufReader::new(stdout);
        // pw-dump --monitor outputs JSON arrays separated by newlines.
        // We just need to detect when new output arrives.
        let mut bracket_depth: i32 = 0;

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    // Track bracket depth to detect complete JSON arrays
                    for ch in line.chars() {
                        match ch {
                            '[' => bracket_depth += 1,
                            ']' => bracket_depth -= 1,
                            _ => {}
                        }
                    }

                    // When we've closed a top-level array, a state change happened
                    if bracket_depth == 0 && line.contains(']')
                        && event_tx.send_blocking(MonitorEvent::StreamsChanged).is_err() {
                            log::debug!("Monitor event channel closed, stopping");
                            return;
                        }
                }
                Err(e) => {
                    log::error!("Error reading pw-dump output: {e}");
                    let _ = event_tx.send_blocking(MonitorEvent::Error(e.to_string()));
                    return;
                }
            }
        }

        log::info!("pw-dump process ended");
    }

    /// Fallback: poll wpctl status every 2 seconds.
    fn run_polling_monitor(event_tx: Sender<MonitorEvent>) {
        loop {
            thread::sleep(Duration::from_secs(2));

            if event_tx.send_blocking(MonitorEvent::StreamsChanged).is_err() {
                log::debug!("Monitor event channel closed, stopping poll");
                return;
            }
        }
    }

    /// Shut down the monitor thread.
    pub fn shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The thread will end when the child process dies or the channel closes
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MonitorThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}
