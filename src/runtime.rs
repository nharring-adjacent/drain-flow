use crate::drains::dd_types::RawLog;
use chrono::Duration;
use std::thread;
use timely::dataflow::{operators::input::Handle, ProbeHandle};
use timely::order::Product;

use crate::drains::dd_runtime::build_dataflow_graph_and_get_handles;
use std::sync::mpsc;
use timely::execute::{execute, Config};
use chrono::Utc; // Added import

pub struct DrainFlowRuntime {
    worker: Option<thread::JoinHandle<()>>,
    // Changed input to Option
    input: Option<Handle<Product<Duration, u64>, RawLog>>,
    probe: ProbeHandle<Product<Duration, u64>>,
}

impl DrainFlowRuntime {
    pub fn new() -> Result<Self, String> {
        // Using two channels for clarity, one for input handle, one for probe handle
        let (input_tx, input_rx) = mpsc::channel::<Handle<Product<Duration, u64>, RawLog>>();
        let (probe_tx, probe_rx) = mpsc::channel::<ProbeHandle<Product<Duration, u64>>>();

        let worker_handle = thread::spawn(move || {
            if let Err(e) = execute(Config::thread(), move |worker| {
                match build_dataflow_graph_and_get_handles(worker) {
                    Ok((input_h, probe_h)) => {
                        if input_tx.send(input_h).is_err() {
                            eprintln!("Failed to send input handle: receiver dropped");
                        }
                        if probe_tx.send(probe_h).is_err() {
                            eprintln!("Failed to send probe handle: receiver dropped");
                        }
                    }
                    Err(s) => {
                        // It's good practice to ensure errors from worker are propagated or logged
                        eprintln!("Failed to build dataflow graph: {}", s);
                    }
                }
            }) {
                eprintln!("Timely worker execution failed: {:?}", e);
            }
        });

        // Receive handles from the worker thread
        let input_handle = input_rx.recv().map_err(|e| format!("Failed to receive input handle: {}", e))?;
        let probe_handle = probe_rx.recv().map_err(|e| format!("Failed to receive probe handle: {}", e))?;

        Ok(Self {
            worker: Some(worker_handle),
            // Store as Some(input_handle)
            input: Some(input_handle),
            probe: probe_handle,
        })
    }

    pub fn push_log(&mut self, log_content: String) {
        let timestamp = Utc::now().timestamp_millis() as u64;
        let raw_log = RawLog {
            content: log_content,
            timestamp_ms: timestamp,
        };

        if let Some(input) = self.input.as_mut() {
            input.send(raw_log);
            let next_time = Duration::milliseconds(timestamp as i64 + 1);
            input.advance_to(Product::new(next_time, 0));
        } else {
            // Handle error: input handle is None, perhaps already dropped or never initialized.
            eprintln!("Attempted to push_log, but input handle is not available.");
        }
    }
}

// Implement Drop
impl Drop for DrainFlowRuntime {
    fn drop(&mut self) {
        // 1. Close the input. This signals to the timely dataflow worker that no more data will be sent.
        // Taking the Option and letting it drop will close the handle.
        if let Some(input_handle) = self.input.take() {
            drop(input_handle); // Explicitly drop, though take() then letting it go out of scope also works.
        }

        // 2. Wait for the timely worker thread to complete its execution.
        if let Some(worker_handle) = self.worker.take() {
            match worker_handle.join() {
                Ok(_) => { /* Worker finished successfully */ }
                Err(e) => eprintln!("Timely worker thread panicked: {:?}", e),
            }
        }
    }
}
