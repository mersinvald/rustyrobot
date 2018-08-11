use std::borrow::Cow;
use failure::Error;
use std::marker::PhantomData;
use super::NodeType;

static DELIMITER: &str = ", ";

#[derive(Copy, Clone, Debug)]
pub enum Lang {
    Rust,
}

pub trait AsQuerySegment {
    fn as_query_segment(&self) -> &'static str;
}

impl AsQuerySegment for Lang {
    fn as_query_segment(&self) -> &'static str {
        match self {
            Lang::Rust => "language:Rust"
        }
    }
}

#[derive(Clone, Debug)]
pub struct Query<'a, 'b, N: NodeType> {
    search_for: PhantomData<N>,
    query: Option<Cow<'a, str>>,
    count: u8,
    after: Option<Cow<'b, str>>,
}

impl<'a, 'b, N: NodeType> Query<'a, 'b, N> {
    pub fn builder() -> QueryBuilder<'a, 'b, N> {
        QueryBuilder::default()
    }

    pub fn to_arg_list(&self) -> String {
        let mut list = format!("type: {}, first: {}", N::type_str(), self.count);

        if let Some(ref query) = self.query {
            list.push_str(DELIMITER);
            list.push_str("query: \\\"");
            list.push_str(&query);
            list.push_str("\\\"")
        }

        if let Some(ref after) = self.after {
            list.push_str(DELIMITER);
            list.push_str("after: \\\"");
            list.push_str(&after);
            list.push_str("\\\"")
        }

        list
    }
}

#[derive(Fail, Debug)]
enum QueryBuilderError {
    #[fail(display = "count must be in 1..100, got {}", count)]
    InvalidCount {
        count: u8,
    },
}

#[derive(Clone, Debug)]
pub struct QueryBuilder<'a, 'b, N> {
    search_for: PhantomData<N>,
    query: Option<Cow<'a, str>>,
    count: Option<u8>,
    after: Option<Cow<'b, str>>,
}


impl<'a, 'b, N: NodeType> QueryBuilder<'a, 'b, N> {
    pub fn raw_query(mut self, raw_query: impl Into<Cow<'a, str>>) -> Self {
        let raw_query = raw_query.into();

        // If we already have something in query field, we append
        if let Some(query) = self.query {
            let mut query = query.into_owned();
            query.push_str(DELIMITER);
            query.push_str(&raw_query);
            self.query = Some(Cow::from(query));
        } else {
            self.query = Some(raw_query);
        }

        self
    }

    pub fn lang(mut self, lang: Lang) -> Self {
        // If we already have something in query field, we append
        if let Some(query) = self.query {
            let mut query = query.into_owned();
            query.push_str(DELIMITER);
            query.push_str(lang.as_query_segment());
            self.query = Some(Cow::from(query));
        } else {
            self.query = Some(Cow::from(lang.as_query_segment()));
        }

        self
    }

    pub fn count(mut self, count: u8) -> Self {
        self.count = Some(count);
        self
    }

    pub fn after(mut self, after: impl Into<Cow<'b, str>>) -> Self {
        self.after = Some(after.into());
        self
    }

    pub fn build(self) -> Result<Query<'a, 'b, N>, Error> {
        let count = self.count.unwrap_or(10);
        if count == 0 || count > 100 {
            bail!(QueryBuilderError::InvalidCount { count })
        }

        let query = Query {
            count,
            search_for: self.search_for,
            query: self.query,
            after: self.after
        };

        Ok(query)
    }
}

impl<'a, 'b, N> Default for QueryBuilder<'a, 'b, N> {
    fn default() -> Self {
        QueryBuilder {
            search_for: PhantomData,
            query: None,
            count: None,
            after: None,
        }
    }
}
