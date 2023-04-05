use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use crossbeam_channel::{Receiver, Sender};

pub struct MemPool<T> {
    inner: Arc<InnerPool<T>>,
    counter: RefCell<usize>,
}

pub struct InnerPool<T> {
    buckets_txs: Vec<Sender<T>>,
    buckets_rxs: Vec<Receiver<T>>,
    buckets: usize,
    capacity: usize,
}

impl<T> InnerPool<T> {
    pub fn new<F>(bucket_count: usize, capacity_per_bucket: usize,
                  init_fn: F) -> Arc<Self> where F: Fn() -> T {
        let mut buckets_rxs = Vec::with_capacity(bucket_count);
        let mut buckets_txs = Vec::with_capacity(bucket_count);

        for _ in 0..bucket_count {
            let (tx, rx) = crossbeam_channel::bounded(capacity_per_bucket);

            for _ in 0..capacity_per_bucket {
                tx.try_send(init_fn()).unwrap();
            }

            buckets_rxs.push(rx);
            buckets_txs.push(tx);
        }

        Arc::new(
            Self {
                buckets_rxs,
                buckets_txs,
                capacity: capacity_per_bucket,
                buckets: bucket_count,
            }
        )
    }

    fn try_pull_from_bucket<'a>(self: &'a Arc<Self>, counter: usize) -> Option<MutMemShare<'a, T>> {
        let bucket = counter % self.buckets;

        match self.buckets_rxs[bucket].try_recv() {
            Ok(mem) => {
                Some(MutMemShare {
                    pool: self,
                    mem: Some(mem),
                    bucket,
                })
            }
            Err(_) => {
                None
            }
        }
    }

    fn try_pull_from_bucket_with_fallback<'a, F>(self: &'a Arc<Self>, counter: usize, fallback: F) -> MutMemShare<'a, T>
        where F: Fn() -> T {
        let bucket = counter % self.buckets;

        match self.buckets_rxs[bucket].try_recv() {
            Ok(mem) => {
                MutMemShare {
                    pool: self,
                    mem: Some(mem),
                    bucket,
                }
            }
            Err(_) => {
                MutMemShare {
                    pool: self,
                    mem: Some(fallback()),
                    bucket,
                }
            }
        }
    }

    fn re_attach(&self, bucket: usize, mem: T) {
        let final_bucket = bucket % self.buckets;

        let _ = self.buckets_txs[final_bucket].try_send(mem);
    }
}

impl<T> MemPool<T> {
    pub fn new<F>(bucket_count: usize, capacity_per_bucket: usize,
                  init_fn: F) -> Self where F: Fn() -> T {
        let inner_pool = InnerPool::new(bucket_count, capacity_per_bucket, init_fn);

        Self {
            inner: inner_pool,
            counter: RefCell::new(0),
        }
    }

    pub fn try_pull(& self) -> Option<MutMemShare<'_, T>> {
        let result = self.inner.try_pull_from_bucket(*self.counter.borrow());

        let mut ref_mut = self.counter.borrow_mut();

        *ref_mut = ref_mut.wrapping_add(1);

        result
    }

    pub fn try_pull_with_fallback<F>(&mut self, fallback: F) -> MutMemShare<'_, T> where F: Fn() -> T {
        let result = self.inner.try_pull_from_bucket_with_fallback(*self.counter.borrow(), fallback);

        let mut ref_mut = self.counter.borrow_mut();

        *ref_mut = ref_mut.wrapping_add(1);

        result
    }
}

impl<T> Clone for MemPool<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            counter: RefCell::new(rand::random()),
        }
    }
}

pub trait PooledMem<T> {
    fn detach(self) -> T;
}

pub struct MutMemShare<'a, T> {
    pool: &'a Arc<InnerPool<T>>,
    mem: Option<T>,
    bucket: usize,
}

impl<'a, T> PooledMem<T> for MutMemShare<'a, T> {
    fn detach(mut self) -> T {
        if let Some(mem) = self.mem.take() {
            mem
        } else {
            unreachable!()
        }
    }
}

impl<'a, T> Deref for MutMemShare<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.mem {
            None => { unreachable!() }
            Some(mem) => { mem }
        }
    }
}

impl<'a, T> DerefMut for MutMemShare<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.mem {
            None => { unreachable!() }
            Some(mem) => { mem }
        }
    }
}

impl<'a, T> MutMemShare<'a, T> {
    pub fn freeze(mut self) -> ShareableMem<T> {
        let pool_clone = Arc::clone(self.pool);

        ShareableMem {
            inner: pool_clone,
            mem: self.mem.take(),
            bucket: self.bucket,
        }
    }
}

impl<'a, T> Drop for MutMemShare<'a, T> {
    fn drop(&mut self) {
        match self.mem.take() {
            Some(mem) => {
                self.pool.re_attach(self.bucket, mem);
            }
            None => {
                // Might be a result of a freeze operation
            }
        }
    }
}

pub struct ShareableMem<T> {
    inner: Arc<InnerPool<T>>,
    mem: Option<T>,
    bucket: usize,
}

impl<T> PooledMem<T> for ShareableMem<T> {
    fn detach(mut self) -> T {
        if let Some(mem) = self.mem.take() {
            mem
        } else {
            unreachable!()
        }
    }
}

impl<T> Deref for ShareableMem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.mem {
            Some(mem) => { mem }
            None => { unreachable!() }
        }
    }
}

impl<T> Drop for ShareableMem<T> {
    fn drop(&mut self) {
        match self.mem.take() {
            Some(mem) => {
                self.inner.re_attach(self.bucket, mem);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::MemPool;

    #[test]
    fn assert_simple_functioning() {
        let mem_pool = MemPool::new(1, 1,
                                    || { Vec::<u8>::with_capacity(4096) });

        let option = mem_pool.try_pull();

        assert!(option.is_some());

        assert!(mem_pool.try_pull().is_none());

        {
            let mem = option.unwrap();

            drop(mem);
        }

        assert!(mem_pool.try_pull().is_some());
    }
}