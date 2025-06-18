use crate::drains::dd_runtime::run_dataflow;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use timely::execute::{execute, Config};

pub struct DrainFlowRuntime {
    worker_handle: Option<thread::JoinHandle<()>>,
    log_sender: Sender<String>,
}

impl DrainFlowRuntime {
    pub fn new() -> Self {
        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let worker_handle = thread::spawn({
            let rx_inner = Arc::clone(&rx);
            move || {
                if let Err(e) = execute(Config::thread(), move |worker| {
                    let receiver = Arc::clone(&rx_inner);
                    run_dataflow(worker, receiver);
                }) {
                    eprintln!("Timely worker execution failed: {:?}", e);
                }
            }
        });
        Self {
            worker_handle: Some(worker_handle),
            log_sender: tx,
        }
    }

    pub fn push_log(&self, log_content: String) {
        if self.log_sender.send(log_content).is_err() {
            eprintln!("Worker thread has shut down");
        }
    }
}

impl Default for DrainFlowRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DrainFlowRuntime {
    fn drop(&mut self) {
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}
