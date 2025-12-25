/*

THIS SOFTWARE IS OPEN SOURCE UNDER THE MIT LICENSE

Copyright 2025 Vincent Maciejewski, & M2 Tech
Contact:
v@m2te.ch
mayeski@gmail.com
https://www.linkedin.com/in/vmayeski/
http://m2te.ch/

*/

//! Group Workers Example - Multiple actors sharing a thread pool
//!
//! Demonstrates:
//! - Creating a Group with multiple actors
//! - Using handle_messages! macro for message dispatch
//! - Using a thread pool to process messages
//! - Sending messages to specific actors in the group

use actors::{define_message, handle_messages, ActorContext, ActorRef, Group, Start};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

// Message to request a computation
struct ComputeMessage {
    input: i32,
}
define_message!(ComputeMessage);

// Message with the computation result
struct ResultMessage {
    worker_id: i32,
    input: i32,
    output: i32,
}
define_message!(ResultMessage);

// A lightweight worker actor that performs computations
struct WorkerActor {
    id: i32,
    result_collector: ActorRef,
}

impl WorkerActor {
    fn new(id: i32, result_collector: ActorRef) -> Self {
        WorkerActor {
            id,
            result_collector,
        }
    }

    fn compute(&self, input: i32) -> i32 {
        // Simulate some computation (squaring the input)
        input * input
    }

    fn on_start(&mut self, _msg: &Start, _ctx: &mut ActorContext) {
        println!("Worker {}: Started", self.id);
    }

    fn on_compute(&mut self, msg: &ComputeMessage, ctx: &mut ActorContext) {
        println!("Worker {}: Processing input {}", self.id, msg.input);
        let output = self.compute(msg.input);
        self.result_collector.send(
            Box::new(ResultMessage {
                worker_id: self.id,
                input: msg.input,
                output,
            }),
            ctx.self_ref(),
        );
    }
}

// Register message handlers for WorkerActor
handle_messages!(WorkerActor,
    Start => on_start,
    ComputeMessage => on_compute
);

// Actor that collects results from workers
struct ResultCollector {
    expected_count: i32,
    received_count: Arc<AtomicI32>,
}

impl ResultCollector {
    fn new(expected_count: i32, received_count: Arc<AtomicI32>) -> Self {
        ResultCollector {
            expected_count,
            received_count,
        }
    }

    fn on_result(&mut self, msg: &ResultMessage, _ctx: &mut ActorContext) {
        println!(
            "Collector: Worker {} computed {}^2 = {}",
            msg.worker_id, msg.input, msg.output
        );
        let count = self.received_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= self.expected_count {
            println!("Collector: All {} results received!", self.expected_count);
        }
    }
}

// Register message handlers for ResultCollector
handle_messages!(ResultCollector,
    ResultMessage => on_result
);

fn main() {
    println!("=== Group Workers Example ===");
    println!("This example shows multiple lightweight workers sharing a thread pool.\n");

    // Track completion
    let received_count = Arc::new(AtomicI32::new(0));
    let num_tasks = 10;

    // Create a simple result collector (runs in its own thread for this demo)
    // In a real app, this could also be in the group
    let (collector_tx, collector_rx) = std::sync::mpsc::channel();
    let collector_ref = actors::ActorRef::new(collector_tx, "collector".to_string());

    // Spawn collector thread
    let received_count_clone = received_count.clone();
    let collector_handle = std::thread::spawn(move || {
        let mut collector = ResultCollector::new(num_tasks, received_count_clone);
        let mut ctx = actors::ActorContext::new();
        while let Ok(envelope) = collector_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            collector.dispatch_message(envelope.msg.as_ref(), &mut ctx);
        }
    });

    // Create a group with 4 worker threads and 5 worker actors
    let mut group = Group::new("workers", 4);

    // Add 5 worker actors to the group
    let mut worker_refs = Vec::new();
    for i in 0..5 {
        let worker = WorkerActor::new(i, collector_ref.clone());
        let worker_ref = group.add(&format!("worker_{}", i), Box::new(worker));
        worker_refs.push(worker_ref);
    }

    // Start the group
    group.start();
    println!("Started group with 4 threads and 5 workers\n");

    // Send messages to workers (round-robin)
    for i in 0..num_tasks {
        let worker_idx = (i as usize) % worker_refs.len();
        println!("Sending ComputeMessage({}) to worker_{}", i + 1, worker_idx);
        worker_refs[worker_idx].send(Box::new(ComputeMessage { input: i + 1 }), None);
    }

    // Wait for all results
    println!("\nWaiting for results...\n");
    while received_count.load(Ordering::SeqCst) < num_tasks {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Shutdown
    std::thread::sleep(std::time::Duration::from_millis(200));
    group.end();
    drop(collector_ref);
    let _ = collector_handle.join();

    println!("\n=== Example Complete ===");
}
