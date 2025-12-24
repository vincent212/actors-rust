/*

THIS SOFTWARE IS OPEN SOURCE UNDER THE MIT LICENSE

Copyright 2025 Vincent Maciejewski, & M2 Tech
Contact:
v@m2te.ch
mayeski@gmail.com
https://www.linkedin.com/in/vmayeski/
http://m2te.ch/

*/

//! Message types and traits for the actor framework.
//!
//! All messages must implement the `Message` trait. Use the `message!` macro
//! for easy implementation.

use std::any::Any;

/// Trait for all messages in the actor system.
///
/// Each message type must have a unique ID (0-511 for optimal dispatch performance).
/// Messages must be Send + 'static to be passed between threads.
///
/// # Example
/// ```
/// use actors::define_message;
///
/// struct Ping { count: i32 }
/// define_message!(Ping, 100);
///
/// struct Pong { count: i32 }
/// define_message!(Pong, 101);
/// ```
pub trait Message: Any + Send + 'static {
    /// Returns the unique message ID at runtime (0-511 recommended for fast dispatch)
    fn message_id(&self) -> u16;

    /// For downcasting to concrete type
    fn as_any(&self) -> &dyn Any;

    /// For downcasting to concrete type (mutable)
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Macro to implement the Message trait for a type.
///
/// # Example
/// ```
/// use actors::define_message;
///
/// struct MyMessage {
///     data: String,
/// }
/// define_message!(MyMessage, 42);
/// ```
#[macro_export]
macro_rules! define_message {
    ($name:ty, $id:expr) => {
        impl $crate::Message for $name {
            fn message_id(&self) -> u16 {
                $id
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }
    };
}

// Re-export for use in macro
pub use std::any::Any as StdAny;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMessage {
        value: i32,
    }
    define_message!(TestMessage, 1);

    #[test]
    fn test_message_id() {
        let msg = TestMessage { value: 42 };
        assert_eq!(msg.message_id(), 1);
    }

    #[test]
    fn test_downcast() {
        let msg = TestMessage { value: 42 };
        let any_ref = msg.as_any();
        let downcasted = any_ref.downcast_ref::<TestMessage>().unwrap();
        assert_eq!(downcasted.value, 42);
    }
}
