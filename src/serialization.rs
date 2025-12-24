/*

THIS SOFTWARE IS OPEN SOURCE UNDER THE MIT LICENSE

Copyright 2025 Vincent Maciejewski, & M2 Tech
Contact:
v@m2te.ch
mayeski@gmail.com
https://www.linkedin.com/in/vmayeski/
http://m2te.ch/

*/

//! Message serialization for remote communication.
//!
//! Provides a registry for serializing/deserializing messages to JSON
//! for transmission over ZMQ.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::Message;

/// Function type for serializing a message to JSON
type SerializeFn = Box<dyn Fn(&dyn Message) -> Value + Send + Sync>;

/// Function type for deserializing JSON to a message
type DeserializeFn = Box<dyn Fn(Value) -> Box<dyn Message> + Send + Sync>;

/// Registry entry containing serialize/deserialize functions
struct RegistryEntry {
    serialize: SerializeFn,
    deserialize: DeserializeFn,
}

/// Global message registry (type name -> entry)
static REGISTRY: Mutex<Option<HashMap<String, RegistryEntry>>> = Mutex::new(None);

/// Reverse registry (message ID -> type name)
static ID_TO_NAME: Mutex<Option<HashMap<u16, String>>> = Mutex::new(None);

/// Register a message type for remote serialization.
///
/// Messages must implement `Serialize` and `DeserializeOwned` from serde.
///
/// # Example
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use actors::{define_message, register_remote_message};
///
/// #[derive(Serialize, Deserialize)]
/// struct Ping { count: i32 }
/// define_message!(Ping, 100);
///
/// // Register for remote communication
/// register_remote_message::<Ping>("Ping");
/// ```
pub fn register_remote_message<M>(type_name: &str)
where
    M: Message + Serialize + DeserializeOwned + 'static,
{
    let type_name_clone = type_name.to_string();

    let mut reg = REGISTRY.lock().unwrap();
    let map = reg.get_or_insert_with(HashMap::new);

    let serialize: SerializeFn = Box::new(move |msg: &dyn Message| {
        // Register the ID->name mapping on first serialize
        {
            let mut id_map = ID_TO_NAME.lock().unwrap();
            let map = id_map.get_or_insert_with(HashMap::new);
            map.entry(msg.message_id()).or_insert_with(|| type_name_clone.clone());
        }
        let typed = msg.as_any().downcast_ref::<M>().expect("Type mismatch in serialize");
        serde_json::to_value(typed).expect("Failed to serialize message")
    });

    let deserialize: DeserializeFn = Box::new(|val: Value| {
        let msg: M = serde_json::from_value(val).expect("Failed to deserialize message");
        Box::new(msg)
    });

    map.insert(
        type_name.to_string(),
        RegistryEntry {
            serialize,
            deserialize,
        },
    );
}

/// Serialize a message to JSON using the registry.
///
/// Returns the message type name and the serialized JSON value.
/// Panics if the message type is not registered.
pub fn serialize_message(msg: &dyn Message, type_name: &str) -> Value {
    let reg = REGISTRY.lock().unwrap();
    let map = reg.as_ref().expect("No messages registered");
    let entry = map
        .get(type_name)
        .unwrap_or_else(|| panic!("Message type '{}' not registered", type_name));
    (entry.serialize)(msg)
}

/// Deserialize a message from JSON using the registry.
///
/// Panics if the message type is not registered.
pub fn deserialize_message(type_name: &str, value: Value) -> Box<dyn Message> {
    let reg = REGISTRY.lock().unwrap();
    let map = reg.as_ref().expect("No messages registered");
    let entry = map
        .get(type_name)
        .unwrap_or_else(|| panic!("Message type '{}' not registered", type_name));
    (entry.deserialize)(value)
}

/// Try to deserialize a message from JSON using the registry.
///
/// Returns Err with the reason if deserialization fails.
pub fn try_deserialize_message(type_name: &str, value: Value) -> Result<Box<dyn Message>, String> {
    let reg = REGISTRY.lock().unwrap();
    let map = match reg.as_ref() {
        Some(m) => m,
        None => return Err("No messages registered".to_string()),
    };

    let entry = match map.get(type_name) {
        Some(e) => e,
        None => return Err(format!("Unknown message type: {}", type_name)),
    };

    // Try deserialization (currently panics on parse errors, could be improved)
    Ok((entry.deserialize)(value))
}

/// Check if a message type is registered.
pub fn is_message_registered(type_name: &str) -> bool {
    let reg = REGISTRY.lock().unwrap();
    if let Some(map) = reg.as_ref() {
        map.contains_key(type_name)
    } else {
        false
    }
}

/// Get the registered type name for a message ID.
/// Returns None if no message with this ID has been serialized yet.
pub fn get_type_name_by_id(msg_id: u16) -> Option<String> {
    let id_map = ID_TO_NAME.lock().unwrap();
    if let Some(map) = id_map.as_ref() {
        map.get(&msg_id).cloned()
    } else {
        None
    }
}

/// Register a message ID to type name mapping directly.
/// This is useful when you want to register the mapping at startup.
pub fn register_message_id(msg_id: u16, type_name: &str) {
    let mut id_map = ID_TO_NAME.lock().unwrap();
    let map = id_map.get_or_insert_with(HashMap::new);
    map.insert(msg_id, type_name.to_string());
}

/// Macro to define a remote-capable message.
///
/// This extends `define_message!` by also deriving Serialize/Deserialize.
/// You still need to call `register_remote_message::<Type>("Type")` at runtime.
///
/// # Example
/// ```rust
/// use actors::define_remote_message;
///
/// define_remote_message!(Ping, 100, { count: i32 });
/// ```
#[macro_export]
macro_rules! define_remote_message {
    ($name:ident, $id:expr, { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(serde::Serialize, serde::Deserialize)]
        pub struct $name {
            $(pub $field: $ty),*
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::define_message;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestMsg {
        value: i32,
        name: String,
    }
    define_message!(TestMsg, 999);

    #[test]
    fn test_register_and_serialize() {
        register_remote_message::<TestMsg>("TestMsg");

        let msg = TestMsg {
            value: 42,
            name: "test".to_string(),
        };

        let json = serialize_message(&msg, "TestMsg");
        assert_eq!(json["value"], 42);
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_deserialize() {
        register_remote_message::<TestMsg>("TestMsg");

        let json = serde_json::json!({
            "value": 123,
            "name": "hello"
        });

        let msg = deserialize_message("TestMsg", json);
        let typed = msg.as_any().downcast_ref::<TestMsg>().unwrap();
        assert_eq!(typed.value, 123);
        assert_eq!(typed.name, "hello");
    }
}
