use json;
use rocksdb::{DB, ColumnFamily, IteratorMode};
use failure::Error;
use std::borrow::Borrow;
use std::str;
use std::fmt::Debug;
use serde::{
    Serialize,
    de::DeserializeOwned
};

pub trait QueueElement: Default + Serialize + DeserializeOwned + Debug {
    const QUEUE_CF: &'static str;
    const ON_DELETE_CF: Option<&'static str>;
    fn empty_with_id<T: Into<String>>(id: T) -> Self;
    fn key(&self) -> &str;
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Fork {
    id: String,
    done: bool,
    fork_url: String,
    fork_ssh_url: String,
}

impl QueueElement for Fork {
    const QUEUE_CF: &'static str = super::cf::FORK_QUEUE;
    const ON_DELETE_CF: Option<&'static str> = Some(super::cf::CREATED_FORKS);

    fn empty_with_id<T: Into<String>>(id: T) -> Self {
        Fork {
            id: id.into(),
            ..Default::default()
        }
    }

    fn key(&self) -> &str {
        &self.id
    }
}


pub fn enqueue<T>(db: &DB, element: &T) -> Result<(), Error>
    where T: QueueElement,
{
    debug!("enqueuing into {}: {:?}", T::QUEUE_CF, element);
    let cf = get_queue_cf::<T>(db);
    match db.get_cf(cf, element.key().as_bytes())? {
        Some(_) => raise!(QueueError::DuplicateEntry),
        None => db.put_cf(cf, element.key().as_bytes(), json::to_string(element)?.as_bytes())?,
    }
    Ok(())
}

pub fn peek<T: QueueElement>(db: &DB) -> Result<Option<T>, Error> {
    trace!("peeking from {}", T::QUEUE_CF);
    let cf = get_queue_cf::<T>(db);
    let kvbytes = db.iterator_cf(cf, IteratorMode::Start)?.next();
    if let Some((key, value)) = kvbytes {
        match json::from_slice(&value) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                error!("{}: failed to parse value {}. Removing the entry.", T::QUEUE_CF, String::from_utf8_lossy(&key));
                delete_without_moving::<T>(db, &key)?;
                // Yeah, I know. Recursion is bad.
                peek(db)
            }
        }
    } else {
        Ok(None)
    }
}

pub fn delete<T>(db: &DB, element: &T) -> Result<(), Error>
    where T: QueueElement
{
    let element = element.borrow();
    delete_without_moving::<T>(db, element.key().as_bytes())?;
    if let Some(cf) = get_on_delete_cf::<T>(db) {
        db.put_cf(cf, element.key().as_bytes(), json::to_string(element)?.as_bytes())?
    }
    Ok(())
}

fn delete_without_moving<T: QueueElement>(db: &DB, key: &[u8]) -> Result<(), Error> {
    let cf = get_queue_cf::<T>(db);
    db.delete_cf(cf, key)?;
    Ok(())
}

fn get_queue_cf<T: QueueElement>(db: &DB) -> ColumnFamily {
    db.cf_handle(T::QUEUE_CF)
        .unwrap_or_else(|| panic!("ColumnFamily {} not exists in DB", T::QUEUE_CF))
}

fn get_on_delete_cf<T: QueueElement>(db: &DB) -> Option<ColumnFamily> {
    T::ON_DELETE_CF.map(|name|
        db.cf_handle(name)
            .unwrap_or_else(|| panic!("ColumnFamily {} not exists in DB", T::QUEUE_CF)))
}

#[derive(Fail, Debug, Copy, Clone)]
enum QueueError {
    #[fail(display = "attempt to enqueue with key duplication")]
    DuplicateEntry
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::Builder;
    use std::collections::HashSet;
    use failure::Error;
    use super::*;
    use std::path::Path;
    use db::*;

    fn new_path() -> PathBuf {
        let root = Builder::new().prefix("simple-db").tempdir().unwrap();
        fs::create_dir_all(root.path()).unwrap();
        root.path().to_owned()
    }

    fn open_db(path: &Path) -> Result<KV, Error> {
        open_and_init_db::<V1, _>(path)
    }

    fn new_db() -> Result<KV, Error> {
        let root = Builder::new().prefix("simple-db").tempdir().unwrap();
        fs::create_dir_all(root.path()).unwrap();
        open_and_init_db::<V1, _>(root.path())
    }

    #[test]
    fn test_enqueue() {
        let db = new_db().unwrap();
        let fork = Fork {
            id: "id".to_owned(),
            ..Default::default()
        };
        enqueue(&db, &fork).unwrap();
        let cf = get_queue_cf::<Fork>(&db);
        assert_eq!(
            &db.get_cf(cf, "id".as_bytes()).unwrap().unwrap()[..],
            json::to_string(&fork).unwrap().as_bytes()
        );
    }

    #[test]
    #[should_panic]
    fn test_double_enqueue() {
        let db = new_db().unwrap();
        let fork = Fork {
            id: "id".to_owned(),
            ..Default::default()
        };
        enqueue(&db, &fork).unwrap();
        enqueue(&db, &fork).unwrap();
    }

    #[test]
    fn test_peek() {
        let db = new_db().unwrap();
        let fork = Fork {
            id: "id".to_owned(),
            ..Default::default()
        };
        enqueue(&db, &fork).unwrap();
        assert_eq!(
            peek(&db).unwrap(),
            Some(fork)
        );
    }

    #[test]
    fn test_peek_multiple() {
        let db = new_db().unwrap();
        let forks = vec![
            Fork { id: "id1".to_owned(), ..Default::default() },
            Fork { id: "id2".to_owned(), ..Default::default() },
            Fork { id: "id3".to_owned(), ..Default::default() },
        ];

        for fork in &forks {
            enqueue(&db, fork).unwrap();
        }

        let peeked_first: Fork = peek(&db).unwrap().unwrap();

        for _ in 0..10 {
            assert_eq!(
                peek(&db).unwrap(),
                Some(peeked_first.clone())
            )
        }
    }

    #[test]
    fn peek_and_delete() {
        let db = new_db().unwrap();
        let forks = vec![
            Fork { id: "id1".to_owned(), ..Default::default() },
            Fork { id: "id2".to_owned(), ..Default::default() },
            Fork { id: "id3".to_owned(), ..Default::default() },
        ];

        for fork in &forks {
            enqueue(&db, fork).unwrap();
        }

        let mut peeked = vec![];

        for _ in &forks {
            peeked.push(peek(&db).unwrap().unwrap());
            delete(&db, peeked.last().unwrap()).unwrap();
        }

        assert_eq!(
            forks.into_iter().collect::<HashSet<_>>(),
            peeked.into_iter().collect::<HashSet<_>>()
        );
    }

    #[test]
    fn peek_malformed() {
        let db = new_db().unwrap();
        let cf = get_queue_cf::<Fork>(&db);
        let fork = Fork { id: "id2".to_owned(), ..Default::default() };

        db.put_cf(cf, b"id", &[0xD, 0xE]).unwrap();
        db.put_cf(cf, b"id2", json::to_string(&fork).unwrap().as_bytes()).unwrap();

        let mut peeked = HashSet::new();
        peeked.insert(peek::<Fork>(&db).unwrap());
        if let Some(fork) = peeked.iter().next().unwrap() {
            delete(&db, fork).unwrap()
        }
        peeked.insert(peek::<Fork>(&db).unwrap());

        assert_eq!(
            vec![None, Some(fork)].into_iter().collect::<HashSet<_>>(),
            peeked
        )
    }
}
