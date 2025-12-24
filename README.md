# Actors - High-Performance Actor Framework for Rust

A lightweight, high-performance actor framework for building concurrent systems in Rust.

## Features

- **Actor Model**: Independent entities processing messages sequentially
- **Message-Driven**: All communication via typed message passing
- **Thread-Safe**: Each actor runs in its own thread with isolated state
- **Low-Latency**: Designed for high-frequency trading systems
- **Simple API**: Easy to learn, minimal boilerplate
- **Thread Pools**: Group multiple lightweight actors on shared threads

## Quick Start

### 1. Define Messages

```rust
use actors::{Message, message};

// Each message type has a unique ID (0-511 for optimal performance)
struct Ping { count: i32 }
message!(Ping, 100);

struct Pong { count: i32 }
message!(Pong, 101);
```

### 2. Create Actors

```rust
use actors::{Actor, ActorContext, Message};

struct PongActor;

impl Actor for PongActor {
    fn process_message(&mut self, msg: &dyn Message, ctx: &mut ActorContext) {
        if let Some(ping) = msg.as_any().downcast_ref::<Ping>() {
            println!("Received ping {}", ping.count);
            ctx.reply(Box::new(Pong { count: ping.count }));
        }
    }
}
```

### 3. Set Up Manager

```rust
use actors::{Manager, ThreadConfig};

fn main() {
    let mut mgr = Manager::new();

    // Register actors
    let pong_ref = mgr.manage("pong", Box::new(PongActor), ThreadConfig::default());
    let ping_ref = mgr.manage("ping", Box::new(PingActor::new(pong_ref)), ThreadConfig::default());

    mgr.init();  // Start all actors
    // ... run ...
    mgr.end();   // Wait for all actors to finish
}
```

## Core Concepts

### Actor
Base trait for all actors. Implement `process_message()` to handle messages.

### Message
All messages implement the `Message` trait. Use the `message!` macro for easy definition.

### Manager
Manages actor lifecycle, thread creation, CPU affinity, and provides a registry for name-based lookup.

### Group
Run multiple lightweight actors in a shared thread pool.

### ActorRef
Cloneable handle for sending messages to an actor. Acts as the actor's "address".

## Messaging

### Async Send (Fire-and-Forget)
```rust
other_actor.send(Box::new(MyMessage { data: 42 }), ctx.self_ref());
```
Message is queued and processed later by the receiver's thread.

### Sync Send (RPC-style)
```rust
if let Some(reply) = other_actor.fast_send(Box::new(Request {}), ctx.self_ref()) {
    if let Some(response) = reply.as_any().downcast_ref::<Response>() {
        // Handle response
    }
}
```
Caller blocks until handler completes and returns a reply.

### Reply
```rust
fn process_message(&mut self, msg: &dyn Message, ctx: &mut ActorContext) {
    ctx.reply(Box::new(Response { result: 42 }));  // Works for both send() and fast_send()
}
```

## CPU Affinity & Priority

```rust
// Pin actor to CPU core 2 with FIFO scheduling, priority 50
let config = ThreadConfig::new(vec![2], 50, libc::SCHED_FIFO);
mgr.manage("my_actor", Box::new(MyActor), config);
```

## Thread Pools with Group

```rust
use actors::Group;

// Create group with 4 worker threads
let mut group = Group::new("workers", 4);

// Add lightweight actors
group.add("worker_1", Box::new(Worker::new()));
group.add("worker_2", Box::new(Worker::new()));
group.add("worker_3", Box::new(Worker::new()));

group.start();
// ... send messages ...
group.end();
```

## Building

```bash
cargo build
```

### Run Examples

```bash
cargo run --example ping_pong
cargo run --example group_workers
```

### Requirements

- Rust 2021 edition
- libc (for pthread affinity/priority)

## Files

```
src/
  lib.rs       - Public API exports
  actor.rs     - Actor trait and ActorRef
  message.rs   - Message trait and macro
  messages.rs  - Built-in messages (Start, Shutdown, Timeout, Continue)
  manager.rs   - Actor lifecycle manager
  group.rs     - Thread pool for lightweight actors
  timer.rs     - Timer utilities

examples/
  ping_pong.rs      - Basic actor communication
  group_workers.rs  - Thread pool with Group
```

## Documentation

See [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) for detailed internals documentation.

## License

MIT License
