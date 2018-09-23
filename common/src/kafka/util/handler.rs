use rdkafka::{
    message::{BorrowedMessage, OwnedMessage, Message},
    producer::{BaseRecord, BaseProducer},
    consumer::{Consumer, BaseConsumer},
    ClientConfig,
};

use threadpool::{ThreadPool, Builder};
use serde::{Serialize, de::DeserializeOwned};
use failure::{Error, err_msg};
use json;

use std::thread;
use std::time::Duration;
use std::marker::PhantomData;
use std::sync::Arc;

use shutdown::GracefulShutdownHandle;

pub struct Handler<I, O> {
    f: Arc<dyn Fn(I) -> Result<O, Error> + Send + Sync + 'static>,
}

impl<I, O> Clone for Handler<I, O> {
    fn clone(&self) -> Self {
        Handler {
            f: self.f.clone()
        }
    }
}

impl<I, O> Handler<I, O> {
    fn new(f: impl Fn(I) -> Result<O, Error> + Send + Sync + 'static) -> Self {
        Handler {
            f: Arc::new(f),
        }
    }

    fn exec(&self, input: I) -> Result<O, Error> {
        (self.f)(input)
    }
}

pub struct Filter<I> {
    f: Arc<dyn Fn(&I) -> bool + Send + Sync + 'static>,
}

impl<I> Filter<I> {
    fn new(f: impl Fn(&I) -> bool + Send + Sync + 'static) -> Self {
        Filter {
            f: Arc::new(f),
        }
    }

    fn exec(&self, input: &I) -> bool {
        (self.f)(input)
    }
}

impl<I> Clone for Filter<I> {
    fn clone(&self) -> Self {
        Filter {
            f: self.f.clone()
        }
    }
}

pub struct HandlerThreadPool<I, O> {
    pool: ThreadPool,
    group: String,
    input_topic: String,
    output_topic: Option<String>,
    filter: Option<Filter<I>>,
    handler: Handler<I, O>,
    _marker: PhantomData<(I, O)>,
}

impl<I, O> HandlerThreadPool<I, O>
    where I: DeserializeOwned + Send + 'static,
          O: Serialize + Send + 'static
{
    pub fn builder() -> HandlerThreadPoolBuilder<I, O> {
        HandlerThreadPoolBuilder::default()
    }

    pub fn start(self, shutdown: GracefulShutdownHandle) -> Result<(), Error> {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("produce.offset.report", "true")
            .set("message.timeout.ms", "5000")
            .create()?;

        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", "127.0.0.1:9092")
            .set("group.id", &self.group)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "latest")
            .create()?;

        consumer.subscribe(&[self.input_topic.as_ref()])?;

        // start producer polling thread
        let producer_poller_handle = {
            let producer = producer.clone();
            let shutdown = shutdown.clone();
            thread::spawn(move || {
                let lock = shutdown.started("producer poller");
                while !shutdown.should_shutdown() {
                    producer.poll(Duration::from_millis(200));
                }
                producer.flush(Duration::from_secs(60));
            })
        };

        // start polling the consumer
        while !shutdown.should_shutdown() {
            // Filter-out errors
            let message = match consumer.poll(Duration::from_millis(200)) {
                Some(Ok(msg)) => msg,
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
            let message = message.detach();
            let payload = match message.payload() {
                Some(payload) => payload,
                None => {
                    warn!("empty payload");
                    &[]
                }
            };

            // Parse json. By convention all messages must be json
            let payload: I = match json::from_slice(payload) {
                Ok(payload) => payload,
                Err(e) => {
                    error!("Payload is invalid json: {}", e);
                    continue
                }
            };

            // Filter out by user-defined filter
            if let Some(filter) = self.filter.clone() {
                if !filter.exec(&payload) {
                    continue
                }
            }

            // Spawn handler job on the pool
            let producer = producer.clone();
            let message_key = message.key().unwrap().to_vec();
            let handler = self.handler.clone();
            let out_topic = self.output_topic.clone();
            self.pool.execute(move || {
                // Call user-defined handler
                let result = match handler.exec(payload) {
                    Ok(result) => result,
                    Err(e) => {
                        error!("Handler failed: {}", e);
                        return;
                    }
                };

                // Encode result
                let json = match json::to_vec(&result) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to encode json");
                        return;
                    }
                };

                // Send retry loop (note that it only guarantees putting message into memory buffer)
                if let Some(out_topic) = out_topic {
                    loop {
                        match producer.send(BaseRecord::to(&out_topic)
                                                    .key(&message_key)
                                                    .payload(&json))
                            {
                                Ok(()) => break,
                                Err((e, _)) => {
                                    warn!("Failed to enqueue, retrying");
                                    thread::sleep(Duration::from_millis(100));
                                },
                            }
                    }
                }
            })
        }

        self.pool.join();
        producer_poller_handle.join();

        Ok(())
    }
}

pub struct HandlerThreadPoolBuilder<I, O> {
    n_threads: Option<usize>,
    group: Option<String>,
    input_topic: Option<String>,
    output_topic: Option<String>,
    filter: Option<Filter<I>>,
    handler: Option<Handler<I, O>>,
    _marker: PhantomData<(I, O)>,
}

impl<I, O> HandlerThreadPoolBuilder<I, O>
    where I: DeserializeOwned,
          O: Serialize
{
    pub fn pool_size(mut self, n: usize) -> Self {
        self.n_threads = Some(n);
        self
    }

    pub fn subscribe(mut self, topic: impl AsRef<str>) -> Self {
        self.input_topic = Some(topic.as_ref().to_owned());
        self
    }

    pub fn group(mut self, group: impl AsRef<str>) -> Self {
        self.group = Some(group.as_ref().to_owned());
        self
    }

    pub fn filter(mut self, filter: impl Fn(&I) -> bool + Send + Sync + 'static) -> Self {
        self.filter = Some(Filter::new(filter));
        self
    }

    pub fn handler(mut self, handler: impl Fn(I) -> Result<O, Error> + Send + Sync + 'static) -> Self {
        self.handler = Some(Handler::new(handler));
        self
    }

    pub fn build(self) -> Result<HandlerThreadPool<I, O>, Error> {
        let pool = if let Some(n_threads) = self.n_threads {
            Builder::new()
                .num_threads(n_threads)
                .build()
        } else {
            Builder::new()
                .build()
        };

        let group = self.group.ok_or(
            err_msg("Group ID is undefined")
        )?;

        let input_topic = self.input_topic.ok_or(
            err_msg("No topic to subscribe")
        )?;

        let output_topic = self.output_topic;

        let filter = self.filter;

        let handler = self.handler.ok_or(
            err_msg("No handler function")
        )?;

        Ok(
            HandlerThreadPool {
                pool,
                group,
                input_topic,
                output_topic,
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
            n_threads: None,
            group: None,
            input_topic: None,
            output_topic: None,
            filter: None,
            handler: None,
        }
    }
}
