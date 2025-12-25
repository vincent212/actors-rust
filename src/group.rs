/*

THIS SOFTWARE IS OPEN SOURCE UNDER THE MIT LICENSE

Copyright 2025 Vincent Maciejewski, & M2 Tech
Contact:
v@m2te.ch
mayeski@gmail.com
https://www.linkedin.com/in/vmayeski/
http://m2te.ch/

*/

//! Group - Thread pool for running multiple lightweight actors.
//!
//! Use a Group when you have many lightweight actors that don't need
//! separate threads. N worker threads process messages for M actors.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::actor::{ActorContext, ActorRef, Envelope};
use crate::messages::{Shutdown, Start};
use crate::Actor;

/// A lightweight actor wrapper for the Group
struct GroupMember {
    actor: Box<dyn Actor>,
    context: ActorContext,
}

impl GroupMember {
    fn new(actor: Box<dyn Actor>, actor_ref: ActorRef) -> Self {
        let mut context = ActorContext::new();
        context.set_self_ref(actor_ref);
        GroupMember {
            actor,
            context,
        }
    }

    fn dispatch(&mut self, envelope: Envelope) {
        let msg = envelope.msg;

        // Set up context for this message
        self.context.prepare_for_envelope(envelope.sender, envelope.reply_channel);

        // Call the actor's process_message
        self.actor.process_message(msg.as_ref(), &mut self.context);
    }
}

/// Envelope with destination actor name for Group routing
struct GroupEnvelope {
    destination: String,
    envelope: Envelope,
}

/// Group - Run multiple lightweight actors in a thread pool.
///
/// Workers pull messages from a shared queue and dispatch to the
/// appropriate actor. Per-actor locks prevent concurrent access.
///
/// # Example
/// ```
/// use actors::{Group, Actor, ActorContext, ActorRef};
///
/// struct LightActor { id: i32 }
/// impl Actor for LightActor {}
///
/// let mut group = Group::new("my_group", 4);  // 4 worker threads
/// let ref1 = group.add("actor1", Box::new(LightActor { id: 1 }));
/// let ref2 = group.add("actor2", Box::new(LightActor { id: 2 }));
/// group.start();
///
/// // Send messages to actors in the group
/// ref1.send(Box::new(MyMessage { ... }), None);
/// ```
pub struct Group {
    name: String,
    num_workers: usize,
    /// Sender for the shared queue
    sender: Sender<GroupEnvelope>,
    /// Receiver for workers (shared)
    receiver: Arc<Mutex<Receiver<GroupEnvelope>>>,
    /// Actor name -> locked actor
    actors: Arc<HashMap<String, Arc<Mutex<GroupMember>>>>,
    /// Mutable map for building during add()
    actors_builder: HashMap<String, Arc<Mutex<GroupMember>>>,
    /// Worker threads
    workers: Vec<JoinHandle<()>>,
    /// Actor refs for shutdown
    actor_refs: Vec<ActorRef>,
    /// Running flag
    running: Arc<Mutex<bool>>,
}

impl Group {
    /// Create a new Group with the specified number of worker threads.
    pub fn new(name: &str, num_workers: usize) -> Self {
        let (sender, receiver) = channel();
        Group {
            name: name.to_string(),
            num_workers,
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            actors: Arc::new(HashMap::new()),
            actors_builder: HashMap::new(),
            workers: Vec::new(),
            actor_refs: Vec::new(),
            running: Arc::new(Mutex::new(true)),
        }
    }

    /// Add an actor to the group.
    ///
    /// Returns an ActorRef for sending messages to the actor.
    pub fn add(&mut self, name: &str, actor: Box<dyn Actor>) -> ActorRef {
        // Create a sender that routes through the group
        let (_tx, _rx) = channel::<Envelope>(); // Dummy channel, we'll use group's sender
        let actor_ref = create_group_actor_ref(
            self.sender.clone(),
            name.to_string(),
        );

        let member = GroupMember::new(actor, actor_ref.clone());
        self.actors_builder.insert(name.to_string(), Arc::new(Mutex::new(member)));
        self.actor_refs.push(actor_ref.clone());

        actor_ref
    }

    /// Get an ActorRef by name.
    pub fn get_ref(&self, name: &str) -> Option<ActorRef> {
        if self.actors_builder.contains_key(name) || self.actors.contains_key(name) {
            Some(create_group_actor_ref(self.sender.clone(), name.to_string()))
        } else {
            None
        }
    }

    /// Get all actor names in this group.
    pub fn get_names(&self) -> Vec<String> {
        self.actors_builder.keys().cloned().collect()
    }

    /// Start the group's worker threads.
    pub fn start(&mut self) {
        // Move actors_builder into actors
        let actors_map: HashMap<_, _> = std::mem::take(&mut self.actors_builder);
        self.actors = Arc::new(actors_map);

        // Initialize all actors
        for (_name, actor_mutex) in self.actors.iter() {
            let mut member = actor_mutex.lock().unwrap();
            member.actor.init();
        }

        // Send Start message to all actors
        for actor_ref in &self.actor_refs {
            actor_ref.send(Box::new(Start), None);
        }

        // Spawn worker threads
        for _i in 0..self.num_workers {
            let receiver = Arc::clone(&self.receiver);
            let actors = Arc::clone(&self.actors);
            let running = Arc::clone(&self.running);

            let handle = thread::spawn(move || {
                worker_loop(receiver, actors, running);
            });
            self.workers.push(handle);
        }
    }

    /// Signal all actors to shut down.
    pub fn shutdown(&self) {
        // Send Shutdown to all actors
        for actor_ref in &self.actor_refs {
            actor_ref.send(Box::new(Shutdown), None);
        }

        // Signal workers to stop
        *self.running.lock().unwrap() = false;
    }

    /// Wait for all worker threads to finish.
    pub fn wait(&mut self) {
        // Call end() on all actors
        for (_, actor_mutex) in self.actors.iter() {
            let mut member = actor_mutex.lock().unwrap();
            member.actor.end();
        }

        // Drop sender to unblock workers
        // Workers will exit when running is false and receiver times out
        let workers = std::mem::take(&mut self.workers);
        for handle in workers {
            let _ = handle.join();
        }
    }

    /// Shutdown and wait for the group.
    pub fn end(&mut self) {
        self.shutdown();
        self.wait();
    }

    /// Get the group name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Worker loop - pulls messages from shared queue and dispatches
fn worker_loop(
    receiver: Arc<Mutex<Receiver<GroupEnvelope>>>,
    actors: Arc<HashMap<String, Arc<Mutex<GroupMember>>>>,
    running: Arc<Mutex<bool>>,
) {
    loop {
        // Check if we should stop
        if !*running.lock().unwrap() {
            // Drain remaining messages
            while let Ok(group_env) = receiver.lock().unwrap().try_recv() {
                dispatch_to_actor(&actors, group_env);
            }
            break;
        }

        // Try to receive a message (with timeout to check running flag)
        let result = {
            let rx = receiver.lock().unwrap();
            rx.try_recv()
        };

        match result {
            Ok(group_env) => {
                dispatch_to_actor(&actors, group_env);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No message, sleep briefly and retry
                std::thread::sleep(std::time::Duration::from_micros(100));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Channel closed, exit
                break;
            }
        }
    }
}

/// Dispatch a message to the appropriate actor
fn dispatch_to_actor(
    actors: &HashMap<String, Arc<Mutex<GroupMember>>>,
    group_env: GroupEnvelope,
) {
    if let Some(actor_mutex) = actors.get(&group_env.destination) {
        let mut member = actor_mutex.lock().unwrap();
        member.dispatch(group_env.envelope);
    }
}

/// Create an ActorRef for Group actors
fn create_group_actor_ref(sender: Sender<GroupEnvelope>, name: String) -> ActorRef {
        // Create a wrapper sender that adds the destination
        let (wrapper_tx, wrapper_rx) = channel::<Envelope>();
        let dest_name = name.clone();
        let group_sender = sender.clone();

        // Spawn a forwarding thread
        std::thread::spawn(move || {
            while let Ok(envelope) = wrapper_rx.recv() {
                let group_env = GroupEnvelope {
                    destination: dest_name.clone(),
                    envelope,
                };
                if group_sender.send(group_env).is_err() {
                    break;
                }
            }
        });

    ActorRef::new(wrapper_tx, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::define_message;
    use crate::Message;
    use std::sync::atomic::{AtomicI32, Ordering};

    struct TestMessage {
        value: i32,
    }
    define_message!(TestMessage);

    struct CountingActor {
        count: Arc<AtomicI32>,
    }

    impl Actor for CountingActor {
        fn init(&mut self) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn process_message(&mut self, msg: &dyn Message, _ctx: &mut ActorContext) {
            if let Some(m) = msg.as_any().downcast_ref::<TestMessage>() {
                self.count.fetch_add(m.value, Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn test_group_creation() {
        let group = Group::new("test_group", 4);
        assert_eq!(group.name(), "test_group");
        assert_eq!(group.num_workers, 4);
    }

    #[test]
    fn test_group_add_actors() {
        let mut group = Group::new("test_group", 2);
        let count = Arc::new(AtomicI32::new(0));

        struct DummyActor;
        impl Actor for DummyActor {}

        let ref1 = group.add("actor1", Box::new(DummyActor));
        let ref2 = group.add("actor2", Box::new(DummyActor));

        assert!(group.get_ref("actor1").is_some());
        assert!(group.get_ref("actor2").is_some());
        assert!(group.get_ref("actor3").is_none());

        assert_eq!(ref1.name(), "actor1");
        assert_eq!(ref2.name(), "actor2");
    }
}
