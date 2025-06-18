use crate::drains::dd_types::RawLog;
use std::sync::{mpsc::Receiver, Arc, Mutex};
use timely::communication::allocator::generic::Generic;
use timely::dataflow::operators::input::Input;
use timely::dataflow::operators::Inspect;
use timely::dataflow::operators::Probe;
use timely::worker::Worker;

pub fn run_dataflow(worker: &mut Worker<Generic>, receiver: Arc<Mutex<Receiver<String>>>) {
    let (mut input, probe) = worker.dataflow::<usize, _, _>(|scope| {
        let (input, stream) = scope.new_input::<RawLog>();
        let probe = stream.inspect(|l| println!("LOG: {:?}", l)).probe();
        (input, probe)
    });

    let mut idx = 0usize;
    loop {
        let line = match receiver.lock().unwrap().recv() {
            Ok(l) => l,
            Err(_) => break,
        };
        input.send(RawLog {
            id: idx,
            content: line,
        });
        input.advance_to(idx + 1);
        worker.step_while(|| probe.less_than(input.time()));
        idx += 1;
    }
}
