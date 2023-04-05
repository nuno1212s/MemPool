use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MemPool<T> {
    inner: Arc<InnerPool<T>>,
    counter: RefCell<usize>,
}

pub struct InnerPool<T> {
    buckets: Vec<Mutex<Vec<T>>>,
    capacity: usize,
}

impl<T> InnerPool<T> {
    pub fn new<F>(bucket_count: usize, capacity_per_bucket: usize,
                  init_fn: F) -> Arc<Self> where F: Fn() -> T {
        let mut buckets = Vec::with_capacity(bucket_count);

        for _ in 0..bucket_count {
            let mut vec = Vec::with_capacity(capacity_per_bucket);

            for _ in 0..capacity_per_bucket {
                vec.push(init_fn());
            }

            buckets.push(Mutex::new(vec));
        }

        Arc::new(
            Self {
                buckets,
                capacity: capacity_per_bucket,
            }
        )
    }

    fn try_pull_from_bucket<'a>(self: &'a Arc<Self>, counter: usize) -> Option<MutMemShare<'a, T>> {
        let bucket = counter % self.buckets.len();

        let mem = {
            let mut bucket_guard = self.buckets[bucket].lock().unwrap();

            bucket_guard.pop()
        };

        mem.map(|mem| MutMemShare {
            pool: self,
            mem: Some(mem),
            bucket,
        })
    }

    fn try_pull_from_bucket_with_fallback<'a, F>(self: &'a Arc<Self>, counter: usize, fallback: F) -> MutMemShare<'a, T>
        where F: Fn() -> T {
        let bucket = counter % self.buckets.len();

        let mem = {
            let mut bucket_guard = self.buckets[bucket].lock().unwrap();

            bucket_guard.pop()
        };

        match mem {
            None => {
                MutMemShare {
                    pool: self,
                    mem: Some(fallback()),
                    bucket,
                }
            }
            Some(mem) => {
                MutMemShare {
                    pool: self,
                    mem: Some(mem),
                    bucket,
                }
            }
        }
    }

    fn re_attach(&self, bucket: usize, mem: T) {
        let final_bucket = bucket % self.buckets.len();

        let mut bucket_guard = self.buckets[final_bucket].lock().unwrap();

        if bucket_guard.len() < self.capacity {
            bucket_guard.push(mem);
        }
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
}


#[cfg(test)]
mod tests {
    use crate::MemPool;

    #[test]
    fn assert_simple_functioning() {
        let mut mem_pool = MemPool::new(1, 1,
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

    #[test]
    fn detach_then_attach() {
        /*
        let pool = Pool::new(1, || Vec::new());
        let (pool, mut object) = pool.try_pull().unwrap().detach();
        object.push(1);
        pool.attach(object);
        assert_eq!(pool.try_pull().unwrap()[0], 1);
         */
    }

    #[test]
    fn pull() {
        /*
        let pool = Pool::<Vec<u8>>::new(1, || Vec::new());

        let object1 = pool.try_pull();
        let object2 = pool.try_pull();
        let object3 = pool.pull(|| Vec::new());

        assert!(object1.is_some());
        assert!(object2.is_none());
        drop(object1);
        drop(object2);
        drop(object3);
        assert_eq!(pool.len(), 2);
        */
    }

    #[test]
    fn e2e() {
        /*
        let pool = Pool::new(10, || Vec::new());
        let mut objects = Vec::new();

        for i in 0..10 {
            let mut object = pool.try_pull().unwrap();
            object.push(i);
            objects.push(object);
        }

        assert!(pool.try_pull().is_none());
        drop(objects);
        assert!(pool.try_pull().is_some());

        for i in (10..0).rev() {
            let mut object = pool.objects.lock().pop().unwrap();
            assert_eq!(object.pop(), Some(i));
        }*/
    }
}