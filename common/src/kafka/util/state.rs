use rdkafka::{
    message::{BorrowedMessage, Message},
    consumer::{Consumer, BaseConsumer},
    error::KafkaError,
    ClientConfig,
};

use kafka::util::producer::ThreadedProducer;
use shutdown::GracefulShutdown;

use std::ops::{Index, IndexMut};

use failure::{Error, err_msg};
use json::{self, Value};
use uuid;

use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::str;

pub type State = HashMap<String, Value>;
pub type StateChange = (String, Value);

pub struct StateHandler {
    old: State,
    new: State,
    topic: String,
    producer: ThreadedProducer,
    shutdown: GracefulShutdown,
}

impl StateHandler {
    pub fn new(topic: impl AsRef<str>,) -> Result<Self, Error> {
        let topic = topic.as_ref().to_owned();
        let shutdown = GracefulShutdown::new();
        let producer = ThreadedProducer::new(&topic, shutdown.thread_handle())?;
        Ok(
            StateHandler {
                old: HashMap::new(),
                new: HashMap::new(),
                topic,
                producer,
                shutdown
            }
        )
    }

    pub fn restore(&mut self) -> Result<(), Error> {
        let group = format!("{}", uuid::Uuid::new_v4());
        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("group.id", &group)
            .set("enable.partition.eof", "true")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .create()?;

        consumer.subscribe(&[self.topic.as_ref()])?;

        for message in &consumer {
            let message = match message {
                Ok(message) => message,
                Err(KafkaError::PartitionEOF(_)) => {
                    info!("restored from {}", self.topic);
                    break;
                },
                Err(e) => Err(e)?
            };
            let (key, value) = message.into_state_change()?;
            debug!("restoring state from {}: {} => {}", self.topic, key, value);
            self.old.insert(key, value);
        }

        self.new = self.old.clone();

        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        info!("synchronizing state changes");
        let delta = self.delta();
        debug!("sync delta size: {}", delta.len());
        trace!("sync delta: {:?}", delta);

        for (key, value) in delta {
            loop {
                match self.producer.send_with_key(key.clone(), value.clone()) {
                    Ok(()) => break,
                    Err(e) => {
                        error!("failed to synchronize state: {}", e);
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        }

        self.old = self.new.clone();
        debug!("state sync finished");
        Ok(())
    }

    pub fn get<S, V>(&self, key: S) -> V
        where S: AsRef<str>,
              V: FromJsonValue
    {
        V::from_json_value(self[key].clone())
    }

    pub fn get_or_default<S, V>(&self, key: S) -> V
        where S: AsRef<str>,
              V: FromJsonValue + Default
    {
        if let Some(value) = self.new.get(key.as_ref()) {
            V::from_json_value(value.clone())
        } else {
            V::default()
        }
    }

    pub fn set<S, V>(&mut self, key: S, value: V)
        where S: AsRef<str>,
              V: Into<Value>
    {
        let value = value.into();
        self[key] = value;
    }

    pub fn set_and_sync<S, V>(&mut self, key: S, value: V) -> Result<(), Error>
        where S: AsRef<str>,
              V: Into<Value>
    {
        self.set(key, value);
        self.sync()
    }

    pub fn increment<S>(&mut self, key: S)
        where S: AsRef<str>
    {
        let key = key.as_ref().to_string();

        let old = self.new.get(&key)
            .cloned()
            .map(i64::from_json_value)
            .unwrap_or(0);

        let new = Value::from(old + 1);

        self.new.insert(key, new);
    }

    fn delta(&self) -> Vec<StateChange> {
        let mut changes = Vec::new();

        for (new_key, new_value) in &self.new {
            let changed = if let Some(old_value) = self.old.get(new_key) {
                new_value != old_value
            } else {
                true
            };

            if changed {
                let change = (
                    new_key.to_owned(),
                    new_value.to_owned(),
                );

                changes.push(change);
            }
        }

        changes
    }
}

impl Drop for StateHandler {
    fn drop(&mut self) {
        self.sync()
            .expect("Failed to synchronize changes");
        self.shutdown.shutdown();
    }
}

impl<S> Index<S> for StateHandler
    where S: AsRef<str>
{
    type Output = Value;
    fn index(&self, idx: S) -> &Self::Output {
        &self.new[idx.as_ref()]
    }
}

impl<S> IndexMut<S> for StateHandler
    where S: AsRef<str>{
    fn index_mut(&mut self, idx: S) -> &mut Self::Output {
        let key = idx.as_ref().to_string();
        self.new.entry(key).or_insert(Value::Null)
    }
}


trait IntoStateChange {
    fn into_state_change(&self) -> Result<StateChange, Error>;
}

trait FromStateChange: Sized {
    fn from_state_change(StateChange) -> Result<Self, Error>;
}

impl<'a> IntoStateChange for BorrowedMessage<'a> {
    fn into_state_change(&self) -> Result<StateChange, Error> {
        let key = self.key().ok_or_else(||
            err_msg("Missing key on state change")
        )?;

        let value = self.payload().ok_or_else(||
            err_msg("Empty state change")
        )?;

        let key = str::from_utf8(key)?;
        let value: Value = json::from_slice(value)?;

        Ok((key.to_owned(), value))
    }
}

pub type KeyValueBytes = (Vec<u8>, Vec<u8>);

impl FromStateChange for KeyValueBytes {
    fn from_state_change((key, value): StateChange) -> Result<Self, Error> {
        let key = key.as_bytes().to_owned();
        let value = json::to_vec(&value)?;
        Ok((key, value))
    }
}

pub trait FromJsonValue {
    fn from_json_value(value: Value) -> Self;
}

impl FromJsonValue for String {
    fn from_json_value(value: Value) -> Self {
        assert!(value.is_string());
        value.as_str().unwrap().to_string()
    }
}

impl FromJsonValue for i64 {
    fn from_json_value(value: Value) -> Self {
        assert!(value.is_i64());
        value.as_i64().unwrap()
    }
}

impl FromJsonValue for f64 {
    fn from_json_value(value: Value) -> Self {
        assert!(value.is_f64());
        value.as_f64().unwrap()
    }
}

impl<T: FromJsonValue> FromJsonValue for Vec<T> {
    fn from_json_value(value: Value) -> Self {
        assert!(value.is_array());
        value.as_array().unwrap()
            .iter()
            .map(|v| T::from_json_value(v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::StateHandler;
    use json::Value;
    use env_logger;
    use uuid::Uuid;

    #[test]
    fn save_and_restore() {
        env_logger::try_init();
        let mut state = StateHandler::new("rustyrobot.test.state.save_and_restore").unwrap();
        state.set("key1", "helloworld");
        state.set("key2", 12345);
        state.set("key3", vec![1, 2, 3, 4, 5]);
        state.sync().unwrap();
        let mut restored = StateHandler::new("rustyrobot.test.state.save_and_restore").unwrap();
        restored.restore().unwrap();
        assert_eq!(state.get::<_, String>("key1"), "helloworld");
        assert_eq!(state.get::<_, i64>("key2"), 12345);
        assert_eq!(state.get::<_, Vec<i64>>("key3"), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn save_and_restore_through_drops() {
        env_logger::try_init();
        let mut last_value = String::new();
        for _ in 0..10 {
            let mut state = StateHandler::new("rustyrobot.test.state.save_and_restore").unwrap();
            state.restore().unwrap();
            if !last_value.is_empty() {
                assert_eq!(state.get::<_, String>("drop"), last_value);
            }
            last_value = Uuid::new_v4().to_string();
            state.set("drop", last_value.clone());
        }
    }

    #[test]
    fn delta() {
        let mut state = StateHandler::new("rustyrobot.test.state.save_and_restore").unwrap();
        state.set("delta_key_1", 1);
        assert_eq!(state.delta(), vec![
            (String::from("delta_key_1"), Value::from(1))
        ]);

        state.set("delta_key_2", 2);
        assert_eq!(state.delta().len(), 2);
        assert!(state.delta().contains(&(String::from("delta_key_1"), Value::from(1))));
        assert!(state.delta().contains(&(String::from("delta_key_2"), Value::from(2))));

        state.set("delta_key_2", 1);
        assert_eq!(state.delta().len(), 2);
        assert!(state.delta().contains(&(String::from("delta_key_1"), Value::from(1))));
        assert!(state.delta().contains(&(String::from("delta_key_2"), Value::from(1))));

        state.sync().unwrap();
        assert_eq!(state.delta(), vec![]);

        state.set("delta_key_1", 1);
        assert_eq!(state.delta(), vec![]);

        state.set("delta_key_2", 1);
        assert_eq!(state.delta(), vec![]);
        state.set("delta_key_2", 2);
        assert_eq!(state.delta(), vec![
            (String::from("delta_key_2"), Value::from(2))
        ]);
    }
}
