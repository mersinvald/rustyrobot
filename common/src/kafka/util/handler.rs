use rdkafka::{
    message::Message,
    consumer::{Consumer, BaseConsumer, CommitMode},
    ClientConfig,
};

use serde::{Serialize, de::DeserializeOwned};
use failure::{Error, err_msg};
use json;

use std::time::Duration;
use std::marker::PhantomData;
use std::rc::Rc;
use std::cell::RefCell;
use std::fmt::Debug;

use shutdown::GracefulShutdownHandle;
use kafka::util::producer::ThreadedProducer;

pub struct HandlingConsumer<I, O> {
    group: String,
    input_topic: String,
    output_topic: Option<String>,
    filter: Option<Box<dyn Fn(&I) -> bool>>,
    key: Option<Box<dyn Fn(&O) -> Vec<u8>>>,
    handler: Box<dyn Fn(I, &mut dyn FnMut(O)) -> Result<(), HandlerError>>,
    _marker: PhantomData<(I, O)>,
}

impl<I, O> HandlingConsumer<I, O>
    where I: DeserializeOwned + Send + Debug + 'static,
          O: Serialize + Send + 'static,
{
    pub fn builder() -> HandlerThreadPoolBuilder<I, O> {
        HandlerThreadPoolBuilder::default()
    }

    pub fn start(self, shutdown: GracefulShutdownHandle) -> Result<(), Error> {
        info!("starting thread-pooled Handler {}/{} -> {:?}", self.input_topic, self.group, self.output_topic);

        let producer = self.output_topic.as_ref().map(|topic| {
            ThreadedProducer::new(&topic, shutdown.clone())
        }).transpose()?;

        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("group.id", &self.group)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", "earliest")
            .set("heartbeat.interval.ms", "1000")
            .create()?;

        consumer.subscribe(&[self.input_topic.as_ref()])?;

        // start polling the consumer
        while !shutdown.should_shutdown() {
            // Filter-out errors
            let borrowed_message = match consumer.poll(Duration::from_millis(200)) {
                Some(Ok(msg)) => {
                    debug!("received message from {}: key: {:?}, offset {}", self.input_topic, msg.key_view::<str>(), msg.offset());
                    msg
                },
                Some(Err(e)) => {
                    warn!("Failed to receive message: {}", e);
                    continue
                },
                None => {
                    trace!("No message");
                    continue
                }
            };

            // Filter out empty messages
            let message = borrowed_message.detach();
            let payload = match message.payload() {
                Some(payload) => payload,
                None => {
                    warn!("empty payload");
                    &[]
                }
            };

            // Parse json. By convention all messages must be json
            let payload: I = match json::from_slice(payload) {
                Ok(payload) => {
                    trace!("payload: {:?}", payload);
                    payload
                },
                Err(e) => {
                    error!("Payload is invalid json: {}", e);
                    consumer.commit_message(&borrowed_message, CommitMode::Sync)?;
                    continue
                }
            };

            // Filter out by user-defined filter
            if let Some(filter) = self.filter.as_ref() {
                if !(filter)(&payload) {
                    trace!("received message filtered out");
                    consumer.commit_message(&borrowed_message, CommitMode::Sync)?;
                    continue
                }
            }

            // Prepare vector to store responses
            let responses = Rc::new(RefCell::new(Vec::new()));

            // Pass message to handler
            {
                let responses = responses.clone();
                let mut callback = move |resp| responses.borrow_mut().push(resp);
                match (self.handler)(payload, &mut callback) {
                    Ok(_) => trace!("message handled successfully"),
                    Err(HandlerError::Other { error }) => {
                        error!("handler failed to process message: {}", error);
                        consumer.commit_message(&borrowed_message, CommitMode::Sync)?;
                        continue;
                    }
                    Err(HandlerError::Internal { error }) => {
                        panic!("internal error, stopping the service without commit: {}", error);
                    }
                }
            }

            // Send responses to the output topic
            for (idx, resp) in responses.borrow().iter().enumerate() {
                let message_key = if let Some(key) = self.key.as_ref() {
                    key(resp)
                } else {
                    let mut base = message.key().unwrap().to_vec();
                    if idx != 0 {
                        base.extend(format!("-{}", idx).as_bytes());
                    }
                    base
                };

                if let Some(producer) = producer.as_ref() {
                    match producer.send_with_key(&message_key, resp) {
                        Ok(()) => (),
                        Err(e) => error!("failed to produce message: {}", e),
                    }
                }
            }

            consumer.commit_message(&borrowed_message, CommitMode::Sync)?;
        }

        Ok(())
    }
}

#[derive(Debug, Fail)]
pub enum HandlerError {
    #[fail(display = "internal error: {}", error)]
    Internal {
        error: Error
    },
    #[fail(display = "{}", error)]
    Other {
        error: Error,
    }
}

impl HandlerError {
    pub fn internal(error: impl Into<Error>) -> Self {
        HandlerError::Internal { error: error.into() }
    }

    pub fn other(error: impl Into<Error>) -> Self {
        HandlerError::Other { error: error.into() }
    }
}

pub struct HandlerThreadPoolBuilder<I, O> {
    group: Option<String>,
    input_topic: Option<String>,
    output_topic: Option<String>,
    filter: Option<Box<dyn Fn(&I) -> bool>>,
    key: Option<Box<dyn Fn(&O) -> Vec<u8>>>,
    handler: Option<Box<dyn Fn(I, &mut dyn FnMut(O)) -> Result<(), HandlerError>>>,
    _marker: PhantomData<(I, O)>,
}

impl<I, O> HandlerThreadPoolBuilder<I, O>
    where I: DeserializeOwned,
          O: Serialize
{
    pub fn subscribe(mut self, topic: impl AsRef<str>) -> Self {
        self.input_topic = Some(topic.as_ref().to_owned());
        self
    }

    pub fn group(mut self, group: impl AsRef<str>) -> Self {
        self.group = Some(group.as_ref().to_owned());
        self
    }

    pub fn respond_to(mut self, topic: impl AsRef<str>) -> Self {
        self.output_topic = Some(topic.as_ref().to_owned());
        self
    }

    pub fn filter(mut self, filter: impl Fn(&I) -> bool + 'static) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn handler(mut self, handler: impl Fn(I, &mut dyn FnMut(O)) -> Result<(), HandlerError> + 'static) -> Self {
        self.handler = Some(Box::new(handler));
        self
    }

    pub fn key_from(mut self, key: impl Fn(&O) -> Vec<u8> + 'static) -> Self {
        self.key = Some(Box::new(key));
        self
    }

    pub fn build(self) -> Result<HandlingConsumer<I, O>, Error> {
        let group = self.group.ok_or_else(||
            err_msg("Group ID is undefined")
        )?;

        let input_topic = self.input_topic.ok_or_else(||
            err_msg("No topic to subscribe")
        )?;

        let output_topic = self.output_topic;

        let filter = self.filter;

        let key = self.key;

        let handler = self.handler.ok_or_else(||
            err_msg("No handler function")
        )?;

        Ok(
            HandlingConsumer {
                group,
                input_topic,
                output_topic,
                key,
                filter,
                handler,
                _marker: self._marker,
            }
        )
    }
}

impl<I, O> Default for HandlerThreadPoolBuilder<I, O> {
    fn default() -> Self {
        HandlerThreadPoolBuilder {
            _marker: PhantomData,
            group: None,
            input_topic: None,
            output_topic: None,
            key: None,
            filter: None,
            handler: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shutdown::GracefulShutdown;
    use std::thread;
    use env_logger;
    use uuid::Uuid;
    use std::sync::{Arc, Mutex};
    use rdkafka::producer::{BaseProducer, BaseRecord};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Payload(String);

    #[test]
    fn interconnection() {
        env_logger::try_init().ok();
        let shutdown = GracefulShutdown::new();

        let supplier = {
            let shutdown = shutdown.thread_handle();
            thread::spawn(move || supplier(shutdown))
        };

        let client_1 = {
            let shutdown = shutdown.thread_handle();
            thread::spawn(move || client("client1", shutdown))
        };

        let client_2 = {
            let shutdown = shutdown.thread_handle();
            thread::spawn(move || client("client2", shutdown))
        };

        thread::sleep(Duration::from_secs(10));
        shutdown.shutdown();
        supplier.join().unwrap();
        client_1.join().unwrap();
        client_2.join().unwrap();
    }

    fn supplier(shutdown: GracefulShutdownHandle) {
        HandlingConsumer::builder()
            .group("handler.test.supplier")
            .subscribe("rustyrobot.test.handler.in")
            .respond_to("rustyrobot.test.handler.out")
            .handler(|message: Payload, callback| {
                callback(message);
                Ok(())
            })
            .build()
            .unwrap()
            .start(shutdown.clone())
            .unwrap();
    }

    fn client(id: &'static str, shutdown: GracefulShutdownHandle) {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()
            .unwrap();

        let send_cnt = 10;

        for _ in 0..send_cnt {
            let payload = Payload(id.to_string());
            let payload = json::to_string(&payload).unwrap();
            producer.send(
                BaseRecord::to("rustyrobot.test.handler.in")
                    .key(&Uuid::new_v4().to_string())
                    .payload(payload.as_bytes())
            ).unwrap();
        }

        producer.flush(Duration::from_secs(10));

        let counter = Arc::new(Mutex::new(0));
        let counter_copy = counter.clone();
        HandlingConsumer::builder()
            .group(&format!("handler.test.client.{}", id))
            .subscribe("rustyrobot.test.handler.out")
            .filter(move |msg: &Payload| msg.0 == id)
            .handler(move |msg: Payload, _callback: &mut dyn FnMut(())| {
                assert_eq!(msg.0, id);
                *counter_copy.lock().unwrap() += 1;
                Ok(())
            })
            .build()
            .unwrap()
            .start(shutdown.clone())
            .unwrap();

        assert_eq!(send_cnt, *counter.lock().unwrap());
    }
}
