use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use futures::lock::Mutex;
use lru::LruCache;
use tokio::sync::oneshot;
use tokio::sync::OwnedRwLockWriteGuard;
use tokio::sync::RwLock;

use self::storage::CacheStorage;
use self::storage::KeyType;
use self::storage::ValueType;
use crate::cache::storage::CacheStorageName;

pub(crate) mod redis;
pub(crate) mod storage;

type WaitMap<K, V> = Arc<Mutex<HashMap<K, Arc<RwLock<Option<V>>>>>>;
pub(crate) const DEFAULT_CACHE_CAPACITY: NonZeroUsize = match NonZeroUsize::new(512) {
    Some(v) => v,
    None => unreachable!(),
};

/// Cache implementation with query deduplication
#[derive(Clone)]
pub(crate) struct DeduplicatingCache<K: KeyType, V: ValueType> {
    wait_map: WaitMap<K, V>,
    memory: Arc<Mutex<LruCache<K, V>>>,
    caller: String,
    storage: CacheStorage<K, V>,
}

impl<K, V> DeduplicatingCache<K, V>
where
    K: KeyType + 'static,
    V: ValueType + 'static,
{
    pub(crate) async fn with_capacity(
        capacity: NonZeroUsize,
        redis_urls: Option<Vec<url::Url>>,
        caller: &str,
    ) -> Self {
        Self {
            wait_map: Arc::new(Mutex::new(HashMap::new())),
            memory: Arc::new(Mutex::new(LruCache::new(capacity))),
            caller: caller.to_string(),
            storage: CacheStorage::new(capacity, redis_urls, caller).await,
        }
    }

    pub(crate) async fn from_configuration(
        config: &crate::configuration::Cache,
        caller: &str,
    ) -> Self {
        Self::with_capacity(
            config.in_memory.limit,
            config.redis.as_ref().map(|c| c.urls.clone()),
            caller,
        )
        .await
    }

    pub(crate) async fn get(&self, key: &K) -> Entry<K, V> {
        match self.get_in_memory(key).await {
            Some(v) => Entry {
                inner: EntryInner::Value(v),
            },
            None => self.get_dedup(key).await,
        }
    }

    async fn get_in_memory(&self, key: &K) -> Option<V> {
        let instant_memory = Instant::now();

        match self.memory.lock().await.get(key).cloned() {
            Some(v) => {
                tracing::info!(
                    monotonic_counter.apollo_router_cache_hit_count = 1u64,
                    kind = %self.caller,
                    storage = &tracing::field::display(CacheStorageName::Memory),
                );
                let duration = instant_memory.elapsed().as_secs_f64();
                tracing::info!(
                    histogram.apollo_router_cache_hit_time = duration,
                    kind = %self.caller,
                    storage = &tracing::field::display(CacheStorageName::Memory),
                );
                Some(v)
            }
            None => {
                let duration = instant_memory.elapsed().as_secs_f64();
                tracing::info!(
                    histogram.apollo_router_cache_miss_time = duration,
                    kind = %self.caller,
                    storage = &tracing::field::display(CacheStorageName::Memory),
                );
                tracing::info!(
                    monotonic_counter.apollo_router_cache_miss_count = 1u64,
                    kind = %self.caller,
                    storage = &tracing::field::display(CacheStorageName::Memory),
                );
                None
            }
        }
    }

    pub(crate) async fn get_dedup(&self, key: &K) -> Entry<K, V> {
        // waiting on a value from the cache is a potentially long(millisecond scale) task that
        // can involve a network call to an external database. To reduce the waiting time, we
        // go through a wait map to register interest in data associated with a key.
        // If the data is present, it is sent directly to all the tasks that were waiting for it.
        // If it is not present, the first task that requested it can perform the work to create
        // the data, store it in the cache and send the value to all the other tasks.
        match self.get_or_insert_wait_map(key).await {
            Err(receiver) => {
                // Register interest in key
                Entry {
                    inner: EntryInner::Receiver { receiver },
                }
            }
            Ok(sender) => {
                let k = key.clone();
                // when _drop_signal is dropped, either by getting out of the block, returning
                // the error from ready_oneshot or by cancellation, the drop_sentinel future will
                // return with Err(), then we remove the entry from the wait map
                let (_drop_signal, drop_sentinel) = oneshot::channel::<()>();
                let wait_map = self.wait_map.clone();
                tokio::task::spawn(async move {
                    let _ = drop_sentinel.await;
                    Self::remove_from_wait_map(&wait_map, &k).await;
                });

                if let Some(value) = self.storage.get(key).await {
                    self.send(sender, key, value.clone()).await;

                    return Entry {
                        inner: EntryInner::Value(value),
                    };
                }

                Entry {
                    inner: EntryInner::First {
                        sender,
                        key: key.clone(),
                        cache: self.clone(),
                        _drop_signal,
                    },
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    async fn get_or_insert_wait_map(
        &self,
        key: &K,
    ) -> Result<OwnedRwLockWriteGuard<Option<V>>, Arc<RwLock<Option<V>>>> {
        let mut locked_wait_map = self.wait_map.lock().await;
        match locked_wait_map.get_mut(key) {
            Some(waiter) => {
                // Register interest in key
                let receiver = waiter.clone();
                drop(locked_wait_map);

                Err(receiver)
            }
            None => {
                let value = Arc::new(RwLock::new(None));
                let w = value
                    .clone()
                    .try_write_owned()
                    .expect("the RwLock was just created");

                locked_wait_map.insert(key.clone(), value);
                drop(locked_wait_map);

                Ok(w)
            }
        }
    }

    async fn remove_from_wait_map(wait_map: &WaitMap<K, V>, key: &K) {
        let mut locked_wait_map = wait_map.lock().await;
        let _ = locked_wait_map.remove(key);
    }

    pub(crate) async fn insert(&self, key: K, value: V) {
        self.insert_in_memory(key.clone(), value.clone()).await;

        self.storage.insert(key, value).await;
    }

    async fn insert_in_memory(&self, key: K, value: V) {
        let size = {
            let mut in_memory = self.memory.lock().await;
            in_memory.put(key, value);
            in_memory.len() as u64
        };
        tracing::info!(
            value.apollo_router_cache_size = size,
            kind = %self.caller,
            storage = &tracing::field::display(CacheStorageName::Memory),
        );
    }

    async fn send(&self, mut sender: OwnedRwLockWriteGuard<Option<V>>, key: &K, value: V) {
        *sender = Some(value);
        drop(sender);

        // Lock the wait map to prevent more subscribers racing with our send
        // notification
        Self::remove_from_wait_map(&self.wait_map, key).await;
    }

    pub(crate) async fn in_memory_keys(&self) -> Vec<K> {
        self.storage.in_memory_keys().await
    }
}

pub(crate) struct Entry<K: KeyType, V: ValueType> {
    inner: EntryInner<K, V>,
}
enum EntryInner<K: KeyType, V: ValueType> {
    First {
        key: K,
        sender: OwnedRwLockWriteGuard<Option<V>>,
        cache: DeduplicatingCache<K, V>,
        _drop_signal: oneshot::Sender<()>,
    },
    Receiver {
        receiver: Arc<RwLock<Option<V>>>,
    },
    Value(V),
}

#[derive(Debug)]
pub(crate) enum EntryError {
    IsFirst,
    NoValue,
}

impl<K, V> Entry<K, V>
where
    K: KeyType + 'static,
    V: ValueType + 'static,
{
    pub(crate) fn is_first(&self) -> bool {
        matches!(self.inner, EntryInner::First { .. })
    }

    pub(crate) async fn get(self) -> Result<V, EntryError> {
        match self.inner {
            // there was already a value in cache
            EntryInner::Value(v) => Ok(v),
            EntryInner::Receiver { receiver } => {
                let r = receiver.read().await;
                (*r).clone().ok_or(EntryError::NoValue)
            }
            _ => Err(EntryError::IsFirst),
        }
    }

    pub(crate) async fn insert(self, value: V) {
        if let EntryInner::First {
            key,
            sender,
            cache,
            _drop_signal,
        } = self.inner
        {
            cache.insert(key.clone(), value.clone()).await;
            cache.send(sender, &key, value).await;
        }
    }

    /// sends the value without storing it into the cache
    #[allow(unused)]
    pub(crate) async fn send(self, value: V) {
        if let EntryInner::First {
            sender, cache, key, ..
        } = self.inner
        {
            cache.send(sender, &key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use futures::stream::FuturesUnordered;
    use futures::stream::StreamExt;
    use mockall::mock;
    use test_log::test;

    use super::DeduplicatingCache;

    #[tokio::test]
    async fn example_cache_usage() {
        let k = "key".to_string();
        let cache =
            DeduplicatingCache::with_capacity(NonZeroUsize::new(1).unwrap(), None, "test").await;

        let entry = cache.get(&k).await;

        if entry.is_first() {
            // potentially long and complex async task that can fail
            let value = "hello".to_string();
            entry.insert(value.clone()).await;
            value
        } else {
            entry.get().await.unwrap()
        };
    }

    #[test(tokio::test)]
    async fn it_should_enforce_cache_limits() {
        let cache: DeduplicatingCache<usize, usize> =
            DeduplicatingCache::with_capacity(NonZeroUsize::new(13).unwrap(), None, "test").await;

        for i in 0..14 {
            let entry = cache.get(&i).await;
            entry.insert(i).await;
        }

        assert_eq!(cache.storage.len().await, 13);
    }

    mock! {
        ResolveValue {
            async fn retrieve(&self, key: usize) -> usize;
        }
    }

    #[test(tokio::test)]
    async fn it_should_only_delegate_once_per_key() {
        let mut mock = MockResolveValue::new();

        mock.expect_retrieve().times(1).return_const(1usize);

        let cache: DeduplicatingCache<usize, usize> =
            DeduplicatingCache::with_capacity(NonZeroUsize::new(10).unwrap(), None, "test").await;

        // Let's trigger 100 concurrent gets of the same value and ensure only
        // one delegated retrieve is made
        let mut computations: FuturesUnordered<_> = (0..100)
            .map(|_| async {
                let entry = cache.get(&1).await;
                if entry.is_first() {
                    let value = mock.retrieve(1).await;
                    entry.insert(value).await;
                    value
                } else {
                    entry.get().await.unwrap()
                }
            })
            .collect();

        while let Some(_result) = computations.next().await {}

        // To be really sure, check there is only one value in the cache
        assert_eq!(cache.storage.len().await, 1);
    }
}
