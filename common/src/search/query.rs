use std::borrow::Cow;
use failure::Error;
use std::marker::PhantomData;
use super::NodeType;

static ARG_LIST_DELIMITER: &str = ", ";
static QUERY_DELIMITER: &str = " ";

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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum SearchFor {
    Repository,
    Undefined
}

impl SearchFor {
    fn type_str(&self) -> &'static str {
        match *self {
            SearchFor::Repository => "",
            SearchFor::Undefined => panic!("Search target is not defined"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Query {
    search_for: SearchFor,
    query: Option<String>,
    count: u8,
    after: Option<String>,
}

impl Query {
    pub fn builder() -> IncompleteQuery {
        IncompleteQuery::default()
    }
}

impl Query {
    pub fn to_arg_list(&self) -> String {
        let mut list = format!("type: {}, first: {}", self.search_for.type_str(), self.count);

        if let Some(ref query) = self.query {
            list.push_str(ARG_LIST_DELIMITER);
            list.push_str("query: \\\"");
            list.push_str(&query);
            list.push_str("\\\"")
        }

        if let Some(ref after) = self.after {
            list.push_str(ARG_LIST_DELIMITER);
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
pub struct IncompleteQuery {
    search_for: SearchFor,
    query: Option<String>,
    count: Option<u8>,
    after: Option<String>,
}


impl IncompleteQuery {
    pub fn raw_query(mut self, raw_query: impl Into<String>) -> Self {
        let raw_query = raw_query.into();

        // If we already have something in query field, we append
        if let Some(query) = self.query {
            let mut query = query.into_owned();
            query.push_str(QUERY_DELIMITER);
            query.push_str(&raw_query);
            self.query = Some(query);
        } else {
            self.query = Some(raw_query);
        }

        self
    }

    pub fn search_for(mut self, t: SearchFor) -> Self {
        self.search_for = t;
        self
    }

    pub fn lang(mut self, lang: Lang) -> Self {
        self.raw_query(lang.as_query_segment())
    }

    pub fn owner(mut self, owner: &str) -> Self {
        self.raw_query(
            format!("user:{}", owner)
        )
    }

    pub fn count(mut self, count: u8) -> Self {
        self.count = Some(count);
        self
    }

    pub fn after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }

    pub fn build(self) -> Result<Query, Error> {
        let count = self.count.unwrap_or(10);
        if count == 0 || count > 100 {
            raise!(QueryBuilderError::InvalidCount { count })
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

impl Default for IncompleteQuery {
    fn default() -> Self {
        IncompleteQuery {
            search_for: PhantomData,
            query: None,
            count: None,
            after: None,
        }
    }
}
