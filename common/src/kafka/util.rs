use rdkafka::{message::{BorrowedMessage, OwnedMessage}};
use threadpool::{ThreadPool, Builder};
use serde::{Serialize, de::DeserializeOwned};
use failure::Error;

pub type Handler<I, O> = Fn(I) -> Result<O, Error>;
pub type Filter<I> = Fn(&I) -> bool;

pub struct HandlerThreadPool<I, O> {
    pool: ThreadPool,
    input_topic: String,
    putput_topic: Option<String>,
    filter: Option<Box<Filter<I>>>,
    handler: Box<Handler<I, O>>,
}

impl<I, O> HandlerThreadPool<I, O>
    where I: DeserializeOwned,
          O: Serialize
{
    pub fn builder() -> HandlerThreadPoolBuilder<I, O> {
        HandlerThreadPoolBuilder::default()
    }

    pub fn start() -> Result<(), Error> {

    }
}

#[derive(Default)]
pub struct HandlerThreadPoolBuilder<I, O> {
    n_threads: Option<usize>,
    input_topic: Option<String>,
    output_topic: Option<String>,
    filter: Option<Box<Filter<I>>>,
    handler: Option<Box<Handler<I, O>>>,
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

    pub fn filter(mut self, filter: impl Filter<I>) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    pub fn handler(mut self, handler: impl Handler<I, O>) -> Self {
        self.handler = Some(Box::new(handler));
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

        let input_topic = self.input_topic.ok_or(
            err_msg!("No topic to subscribe")
        )?;

        let output_topic = self.output_topic;

        let filter = self.filter;

        let handler = self.handler.ok_or(
            err_msg!("No handler function")
        )?;

        Ok(
            HandlerThreadPool {
                pool,
                input_topic,
                putput_topic,
                filter,
                handler
            }
        )
    }
}
