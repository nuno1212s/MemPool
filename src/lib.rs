use std::iter;
use std::sync::{Arc, Mutex};

pub struct MemPool<T, F> where F: Fn() -> T {
    buckets: Vec<Mutex<Vec<T>>>,

    init_fn: F,
}

impl<T, F> MemPool<T, F> where F: Fn() -> T{
    pub fn new(bucket_count: usize, capacity_per_bucket: usize,
               init_fn: F) -> Arc<Self> {
        Arc::new(
            Self {
                buckets: iter::from_fn(Mutex::new(Vec::with_capacity(capacity_per_bucket)))
                    .take(bucket_count)
                    .collect(),
                init_fn,
            }
        )
    }
}