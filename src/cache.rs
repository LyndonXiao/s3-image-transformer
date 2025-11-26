use moka::future::Cache;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct CacheConfig {
    pub max_capacity_mb: u64,
    pub time_to_live_sec: u64,
    pub time_to_idle_sec: u64,
}

#[derive(Clone)]
pub struct ImageCache {
    cache: Arc<Cache<String, Vec<u8>>>,
    config: CacheConfig,
}

impl ImageCache {
    pub fn new(config: CacheConfig) -> Self {
        let max_capacity = config.max_capacity_mb * 1024 * 1024; // 转换为字节
        
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .weigher(|_key, value: &Vec<u8>| -> u32 {
                // 使用字节数作为权重，限制为u32::MAX
                value.len().min(u32::MAX as usize) as u32
            })
            .time_to_live(Duration::from_secs(config.time_to_live_sec))
            .time_to_idle(Duration::from_secs(config.time_to_idle_sec))
            .build();
            
        Self {
            cache: Arc::new(cache),
            config,
        }
    }

    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.cache.get(key)
    }

    pub async fn insert(&self, key: String, value: Vec<u8>) {
        self.cache.insert(key, value).await;
    }

    pub async fn remove(&self, key: &str) {
        self.cache.invalidate(key).await;
    }

    pub async fn clear(&self) {
        self.cache.invalidate_all();
    }

    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }

    pub fn weighted_size(&self) -> u64 {
        self.cache.weighted_size()
    }

    pub fn get_stats(&self) -> CacheStats {
        // 某些 moka 版本上没有公开 stats()，这里暂时返回基本信息并将 hit_rate 置为 0.0
        CacheStats {
            entry_count: self.entry_count(),
            weighted_size: self.weighted_size(),
            max_capacity: self.config.max_capacity_mb * 1024 * 1024,
            hit_rate: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
    pub max_capacity: u64,
    pub hit_rate: f64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let usage_mb = self.weighted_size as f64 / 1024.0 / 1024.0;
        let max_mb = self.max_capacity as f64 / 1024.0 / 1024.0;
        let usage_percent = if max_mb > 0.0 {
            (usage_mb / max_mb) * 100.0
        } else {
            0.0
        };
        
        write!(
            f,
            "CacheStats: entries={}, size={:.2}MB/{:.2}MB ({:.1}%), hit_rate={:.2}%",
            self.entry_count,
            usage_mb,
            max_mb,
            usage_percent,
            self.hit_rate * 100.0
        )
    }
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 仅展示配置，避免打印内部缓存实现的复杂结构
        f.debug_struct("ImageCache")
            .field("config", &self.config)
            .finish()
    }
}