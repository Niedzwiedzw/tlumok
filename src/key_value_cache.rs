// pub use self::cache_service::{
//     CacheFor,
//     ExpiresAfter,
//     GetManyResults,
// };
pub mod cache_service;
use cache_service::{
    CacheFor,
    ExpiresAfter,
    GetManyResults,
};
pub type Key<KV> = <KV as KeyValue>::K;
pub type Value<KV> = <KV as KeyValue>::V;
pub type Lookup<KV> = HashMap<Key<KV>, Value<KV>>;

use super::*;
pub trait KeyValue {
    type K;
    type V;
}

impl<K, V> KeyValue for (K, V) {
    type K = K;
    type V = V;
}

use eyre::{
    Result,
    WrapErr,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::Arc,
};
use tokio::sync::RwLock;
