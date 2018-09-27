use rdkafka::{
    message::{BorrowedMessage, OwnedMessage, Message},
    producer::{BaseRecord, BaseProducer},
    consumer::{Consumer, BaseConsumer},
    message::ToBytes,
    ClientConfig,
};

use threadpool::{ThreadPool, Builder};
use serde::{Serialize, de::DeserializeOwned};
use failure::{Error, err_msg};
use json;
use uuid::Uuid;

use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::marker::PhantomData;
use std::sync::Arc;
use std::rc::Rc;
use std::cell::RefCell;
use std::fmt::Debug;

use shutdown::GracefulShutdownHandle;

pub struct ThreadedProducer {
    producer: BaseProducer,
    topic: String,
    shutdown: GracefulShutdownHandle,
    poller: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct ThreadedProducerHandle {
    producer: BaseProducer,
    topic: String,
}

impl ThreadedProducer {
    pub fn new(topic: impl AsRef<str>, shutdown: GracefulShutdownHandle) -> Result<Self, Error> {
        let topic = topic.as_ref().to_owned();

        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()?;

        // start producer polling thread
        let poller = Some({
            let producer = producer.clone();
            let shutdown = shutdown.clone();
            let thread_description = format!("producer poller for {}", topic);
            thread::spawn(move || {
                let thread_id = format!("{} ({:?})", thread_description, thread::current().id());
                let lock = shutdown.started(thread_id);
                while !shutdown.should_shutdown() {
                    producer.poll(Duration::from_millis(200));
                }
                producer.flush(Duration::from_secs(60));
            })
        });

        Ok(
            ThreadedProducer {
                producer,
                topic,
                poller,
                shutdown,
            }
        )
    }

    pub fn handle(&self) -> ThreadedProducerHandle {
        ThreadedProducerHandle {
            producer: self.producer.clone(),
            topic: self.topic.clone()
        }
    }

    pub fn send<V>(&self, value: V) -> Result<(), Error>
        where V: Serialize
    {
        self.handle().send(value)
    }

    pub fn send_with_key<V>(&self, key: impl ToBytes, value: V) -> Result<(), Error>
        where V: Serialize
    {
        self.handle().send_with_key(key, value)
    }

}

impl ThreadedProducerHandle {
    pub fn send<V>(&self, value: V) -> Result<(), Error>
        where V: Serialize
    {
        let key = Uuid::new_v4().to_string();
        self.send_with_key(key, value)
    }

    pub fn send_with_key<V>(&self, key: impl ToBytes, value: V) -> Result<(), Error>
        where V: Serialize
    {
        let value = json::to_vec(&value)?;
        // Send retry loop (note that it only guarantees putting message into memory buffer)
        loop {
            match self.producer.send(BaseRecord::to(&self.topic)
                .key(&key)
                .payload(&value))
                {
                    Ok(()) => break,
                    Err((e, _)) => {
                        warn!("Failed to enqueue, retrying");
                        thread::sleep(Duration::from_millis(100));
                    },
                }
        }
        debug!("produced message into {}", self.topic);
        Ok(())
    }
}

impl Drop for ThreadedProducer {
    fn drop(&mut self) {
        if !self.shutdown.should_shutdown() {
            error!("called ThreadedProducer::drop is non-shutdown phase, would block forever");
        }
        if let Some(poller) = self.poller.take() {
            poller.join()
                .unwrap_or_else(|err| error!("producer poller thread have panicked: {:?}", err))
        }
    }
}
