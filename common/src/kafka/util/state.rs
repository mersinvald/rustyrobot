use rdkafka::{
    message::{BorrowedMessage, OwnedMessage, Message},
    producer::{BaseRecord, BaseProducer},
    consumer::{Consumer, BaseConsumer},
    ClientConfig,
};

use threadpool::{ThreadPool, Builder};
use serde::{Serialize, de::DeserializeOwned};
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
    consumer: BaseConsumer,
    producer: BaseProducer,
}

impl StateHandler {
    pub fn new(topic: impl AsRef<str>) -> Result<Self, Error> {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()?;

        let group = format!("{}", uuid::Uuid::new_v4());

        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("group.id", &group)
            .set("enable.partition.eof", "true")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "earliest")
            .create()?;

        consumer.subscribe(&[topic.as_ref()])?;

        Ok(
            StateHandler {
                old: HashMap::new(),
                new: HashMap::new(),
                topic: topic.as_ref().to_owned(),
                consumer,
                producer
            }
        )
    }

    pub fn restore(&mut self) -> Result<(), Error> {
        for message in &self.consumer {
            let message = message?;
            let (key, value) = message.into_state_change()?;
            self.old.insert(key, value);
        }
        self.new = self.old.clone();
        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        let delta = self.delta();
        for change in delta {
            let (key, value) = KeyValueBytes::from_state_change(change)?;

            let mut record = BaseRecord::to(&self.topic)
                .key(&key)
                .payload(&value);

            loop {
                match self.producer.send(record) {
                    Ok(_) => break,
                    Err((e, r)) => {
                        warn!("failed to enqueue state change. retrying.");
                        record = r;
                        // flush producer after failure
                        self.producer.flush(Duration::from_secs(60));
                    }
                }
            }
        }
        self.producer.flush(Duration::from_secs(60));
        self.old = self.new.clone();
        Ok(())
    }

    fn delta(&self) -> Vec<StateChange> {
        let mut changes = Vec::new();

        for (new_key, new_value) in &self.new {
            let changed = if let Some((old_key, old_value)) = self.old.get_key_value(new_key) {
                new_value != old_key
            } else {
                true
            };

            if changed {
                let change = ((
                    new_key.to_owned(),
                    new_value.to_owned(),
                ));

                changes.push(change);
            }
        }

        changes
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
        let key = self.key().ok_or(
            err_msg("Missing key on state change")
        )?;

        let value = self.payload().ok_or(
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
