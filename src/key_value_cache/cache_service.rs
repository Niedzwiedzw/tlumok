use crate::translation_service::LanguagePair;

use super::*;
use futures::StreamExt;
use futures::TryStreamExt;
use itertools::Itertools;
use sled::IVec;

#[derive(Clone, Debug)]
pub struct CacheFor<KV> {
    key: PathBuf,
    cache_db: Arc<RwLock<sled::Db>>,
    phantom_data: PhantomData<KV>,
    expires_after: ExpiresAfter,
}

pub fn language_pair_db_key(language_pair: LanguagePair) -> Result<PathBuf> {
    Ok(crate::filesystem::dictionaries_directory()?
        .join(format!("{}-{}", language_pair.0, language_pair.1)))
}

pub fn project_part_of_db_key(project_key: String, type_name: &'static str) -> PathBuf {
    PathBuf::from(&format!("{project_key}")).join(type_name)
}

pub fn dictionary_project_key_language_pair_key(
    language_pair: LanguagePair,
    project_key: String,
    type_name: &'static str,
) -> Result<PathBuf> {
    language_pair_db_key(language_pair)
        .map(|base| base.join(project_part_of_db_key(project_key, type_name)))
}

pub fn original_document_path_project_key(path: &Path) -> String {
    path.components()
        .into_iter()
        .map(|c| c.as_os_str().to_string_lossy())
        .join("_")
}
pub fn project_dictionary(
    original_document_path: &Path,
    language_pair: LanguagePair,
) -> Result<translation_service::TranslationCache> {
    let project_key = original_document_path_project_key(original_document_path);
    // translation_service::TranslationCache::new(
    //     ExpiresAfter::Never,
    // )
    dictionary_at_path(dictionary_project_key_language_pair_key(
        language_pair,
        project_key,
        "dictionary",
    )?)
}
/// this access is unchecked, prefer usage of [project_dictionary]
pub fn dictionary_at_path(path: PathBuf) -> Result<translation_service::TranslationCache> {
    translation_service::TranslationCache::new(path, ExpiresAfter::Never)
}
#[derive(Serialize, Deserialize)]
pub struct CacheEntry<V> {
    pub value: V,
    pub created: crate::AppTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExpiresAfter {
    Never,
    After(std::time::Duration),
}

enum DbGetResult<KV: KeyValue> {
    Return(Value<KV>),
    NotFound,
    Remove(Key<KV>),
}

impl ExpiresAfter {
    pub fn expired(self, created: crate::AppTime) -> Result<bool> {
        let val = match self {
            ExpiresAfter::Never => false,
            ExpiresAfter::After(duration) => {
                (created + chrono::Duration::from_std(duration).context("bad expiry value")?)
                    < crate::now()
            }
        };
        Ok(val)
    }
}

impl<KV: KeyValue> CacheFor<KV>
where
    Key<KV>: Serialize + DeserializeOwned + Clone + std::fmt::Debug,
    Value<KV>: Serialize + DeserializeOwned + std::fmt::Debug,
{
    pub fn new(path: PathBuf, expires_after: ExpiresAfter) -> Result<Self> {
        let base_db_path = crate::filesystem::dictionaries_directory()
            .wrap_err("finding base directory for dictionaries")?;
        Ok(Self {
            expires_after,
            cache_db: Arc::new(RwLock::new(
                // sled::open(format!("./cache_db/{key}/{type_name}.cache")).with_context(|| {
                sled::open(&path)
                    .with_context(|| format!("opening cache database for key {path:?} "))?,
            )),
            key: path,
            phantom_data: Default::default(),
        })
    }

    async fn get_internal(&self, key: Key<KV>) -> Result<Option<CacheEntry<Value<KV>>>> {
        let db = self.cache_db.read().await;
        tokio::task::block_in_place(|| -> Result<_> {
            Ok(
                match db
                    .get(bincode::serialize(&key).context("serializing key")?)
                    .with_context(|| format!("reading db for {:?}", self.key))?
                {
                    Some(v) => Some(
                        bincode::deserialize(&v[..])
                            .with_context(|| format!("deserializing {:?}", self.key))?,
                    ),
                    None => None,
                },
            )
        })
    }

    async fn insert_internal(&self, key: Key<KV>, value: CacheEntry<Value<KV>>) -> Result<()> {
        let db = self.cache_db.write().await;
        tokio::task::block_in_place(|| -> Result<()> {
            db.insert(
                bincode::serialize(&key).context("serializing key")?,
                bincode::serialize(&value).context("serializing value")?,
            )
            .context("inserting data")?;
            Ok(())
        })?;
        Ok(())
    }

    pub async fn insert(&self, key: Key<KV>, value: Value<KV>) -> Result<()> {
        if self.get(key.clone()).await?.is_some() {
            // not sure about that...
            tracing::debug!(
                "[{:?}] [dictionary] value already exists [{:?}]",
                self.key,
                key
            );
            return Ok(());
        }
        tracing::debug!(
            "[{:?}] [dictionary] inserting value [{:?}] on key [{:?}]",
            self.key,
            value,
            key
        );
        self.insert_internal(
            key,
            CacheEntry {
                value,
                created: crate::now(),
            },
        )
        .await
    }
    pub async fn remove(&self, key: Key<KV>) -> Result<()> {
        let db = self.cache_db.write().await;
        tokio::task::block_in_place(|| -> Result<_> {
            tracing::debug!("[{:?}] [dictionary] removing key [{:?}]", self.key, key);
            db.remove(bincode::serialize(&key).context("serializing key for removal")?)
                .context("removing item")?;
            Ok(())
        })?;

        Ok(())
    }
    pub async fn get(&self, key: Key<KV>) -> Result<Option<Value<KV>>> {
        let result: DbGetResult<KV> = match self.get_internal(key.clone()).await? {
            Some(CacheEntry { value, created }) => {
                if self.expires_after.expired(created)? {
                    DbGetResult::Remove(key)
                } else {
                    DbGetResult::Return(value)
                }
            }
            None => DbGetResult::NotFound,
        };

        return Ok(match result {
            DbGetResult::Return(v) => Some(v),
            DbGetResult::NotFound => None,
            DbGetResult::Remove(key) => {
                if let Err(e) = self.remove(key).await {
                    tracing::error!("failed to remove key :: {e}");
                }
                None
            }
        });
    }

    pub async fn get_many(&self, keys: Vec<Key<KV>>) -> Result<GetManyResults<KV>> {
        let found = Vec::with_capacity(keys.len() / 2);
        let not_found_keys = Vec::with_capacity(keys.len() / 2);
        let found_keys = Vec::with_capacity(keys.len() / 2);
        let results: Vec<_> = futures::stream::iter(keys)
            .map(|key| async {
                self.get(key.clone()).await.map(|result| match result {
                    Some(value) => SearchResult::<KV>::Found((key, value)),
                    None => SearchResult::<KV>::NotFound(key),
                })
            })
            .buffer_unordered(1)
            .try_collect()
            .await
            .context("aggregating many results")?;

        let (found, not_found_keys, found_keys) = tokio::task::block_in_place(|| {
            results.into_iter().fold(
                (found, not_found_keys, found_keys),
                |(mut found, mut not_found_keys, mut found_keys), search_result| match search_result
                {
                    SearchResult::Found((key, value)) => {
                        found_keys.push(key.clone());
                        found.push((key, value));
                        (found, not_found_keys, found_keys)
                    }
                    SearchResult::NotFound(v) => {
                        not_found_keys.push(v);
                        (found, not_found_keys, found_keys)
                    }
                },
            )
        });

        Ok(GetManyResults {
            found,
            not_found_keys,
            found_keys,
        })
    }

    pub async fn update<T: IntoIterator<Item = (Key<KV>, Value<KV>)>>(
        &self,
        results: T,
    ) -> Result<()> {
        futures::stream::iter(results)
            .map(|(key, value)| self.insert(key, value))
            .buffer_unordered(1)
            .try_collect()
            .await
            .context("updating results")?;
        Ok(())
    }

    pub async fn get_all(&self) -> Result<GetManyResults<KV>> {
        let results = {
            let db = self.cache_db.read().await;
            tokio::task::block_in_place(|| -> Result<_> {
                db.iter()
                    .map(|v| v.context("getting aggregated result"))
                    .collect::<Result<Vec<_>>>()
            })?
        };
        let found = tokio::task::block_in_place(|| -> Result<_> {
            let found = results
                .into_iter()
                .map(|(key, value): (IVec, IVec)| {
                    bincode::deserialize::<Key<KV>>(&key)
                        .context("deserializing key")
                        .and_then(|key| {
                            bincode::deserialize::<CacheEntry<Value<KV>>>(&value)
                                .context("deserializing value")
                                .map(|value| (key, value))
                        })
                })
                .collect::<Result<Vec<(Key<KV>, CacheEntry<Value<KV>>)>>>()?;

            let found = found
                .into_iter()
                .map(|(key, value)| {
                    self.expires_after
                        .expired(value.created)
                        .map(|expired| ((key, value), expired))
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(found
                .into_iter()
                .filter_map(|(entry, expired)| expired.then_some(entry))
                .map(|(key, value)| (key, value.value))
                .collect_vec())
        })?;

        Ok(GetManyResults {
            found_keys: found.iter().map(|(key, _)| key.clone()).collect(),
            not_found_keys: vec![],
            found,
        })
    }
}
pub struct GetManyResults<KV: KeyValue> {
    pub found: Vec<(Key<KV>, Value<KV>)>,
    pub not_found_keys: Vec<Key<KV>>,
    pub found_keys: Vec<Key<KV>>,
}

enum SearchResult<KV: KeyValue> {
    Found((Key<KV>, Value<KV>)),
    NotFound(Key<KV>),
}
