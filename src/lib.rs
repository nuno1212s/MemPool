use std::iter;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MemPool<T> {
    buckets: Vec<Mutex<Vec<T>>>,

    init_fn: Box<dyn Fn() -> T>,
}

impl<T> MemPool<T> {
    pub fn new<F>(bucket_count: usize, capacity_per_bucket: usize,
                  init_fn: F) -> Arc<Self> where F: Fn() -> T {
        Arc::new(
            Self {
                buckets: iter::from_fn(Mutex::new(Vec::with_capacity(capacity_per_bucket)))
                    .take(bucket_count)
                    .collect(),
                init_fn,
            }
        )
    }


    pub fn try_pull(&self) -> Option<T> {
        todo!()
    }

    fn re_attach(&self, mem: T) {
        todo!()
    }
}

pub struct ModifiableMemShare<T> {
    pool: Arc<MemPool<T>>,

    mem: Option<T>,
}

impl<T> ModifiableMemShare<T> {
    pub fn freeze(self) -> ShareableMemShare<T> {
        ShareableMemShare {
            ref_count: Arc::new(AtomicUsize::new(1)),
            pool: self.pool,
            mem: Some(Arc::new(self.mem)),
        }
    }
}

impl<T> Deref for ModifiableMemShare<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.mem {
            None => { unreachable!() }
            Some(mem) => { mem }
        }
    }
}

impl<T> DerefMut for ModifiableMemShare<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.mem {
            None => { unreachable!() }
            Some(mem) => { mem }
        }
    }
}

impl<T> Drop for ModifiableMemShare<T> {
    fn drop(&mut self) {
        self.pool.re_attach(self.mem.take())
    }
}

pub struct ShareableMemShare<T> {
    ref_count: Arc<AtomicUsize>,
    pool: Arc<MemPool<T>>,
    mem: Option<Arc<T>>,
}

impl<T> Deref for ShareableMemShare<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.mem {
            None => { unreachable!() }
            Some(mem) => { mem }
        }
    }
}

impl<T> Clone for ShareableMemShare<T> {
    fn clone(&self) -> Self {
        self.ref_count.fetch_add(1, Ordering::Relaxed);

        Self {
            ref_count: self.ref_count.clone(),
            pool: self.pool.clone(),
            mem: self.mem.clone(),
        }
    }
}

impl<T> Drop for ShareableMemShare<T> {
    fn drop(&mut self) {
        let counter = self.ref_count.fetch_sub(1, Ordering::Relaxed);

        if counter == 1 {
            let mem = self.mem.take().unwrap();

            if let Ok(mem) = Arc::try_unwrap(mem) {
                self.pool.re_attach(mem);
            } else {
                unreachable!()
            }
        }
    }
}

impl<T> ShareableMemShare<T> {}