use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct MemPool<T> {
    buckets: Vec<Mutex<Vec<T>>>,
    pull_ctr: AtomicUsize,
    insert_ctr: AtomicUsize,
    capacity: usize,
}

impl<T> MemPool<T> {
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
                pull_ctr: AtomicUsize::new(0),
                insert_ctr: AtomicUsize::new(0),
                capacity: capacity_per_bucket,
            }
        )
    }

    fn try_pull_from_bucket(self: &Arc<Self>, bucket: usize) -> Option<ModifiableMemShare<T>> {
        let mem = {
            let mut bucket_guard = self.buckets[bucket].lock().unwrap();

            bucket_guard.pop()
        };

        mem.map(|mem| ModifiableMemShare {
            pool: Arc::clone(self),
            mem: Some(mem),
        })
    }

    #[inline]
    pub fn try_pull(self: &Arc<Self>) -> Option<ModifiableMemShare<T>> {
        let round_robin = self.pull_ctr.fetch_add(1, Ordering::Relaxed);

        let bucket = round_robin % self.buckets.len();

        self.try_pull_from_bucket(bucket)
    }

    /// Try to pull a memory object from all of the buckets
    #[inline]
    pub fn try_pull_from_all_buckets(self: &Arc<Self>) -> Option<ModifiableMemShare<T>> {
        let round_robin = self.pull_ctr.fetch_add(1, Ordering::Relaxed);
        let bucket_len = self.buckets.len();

        let bucket = round_robin % bucket_len;

        let mut bucket_inc = 0;

        loop {
            if bucket_inc >= bucket_len {
                break;
            }

            let final_bucket = (bucket + bucket_inc) % bucket_len;

            if let Some(mem) = self.try_pull_from_bucket(final_bucket) {
                return Some(mem);
            } else {
                bucket_inc += 1;
            }
        }

        None
    }

    /// Try to pull from all of the buckets and if this still fails, then use the provided fallback
    /// function
    #[inline]
    pub fn pull_with_fallback<F>(self: &Arc<Self>, fallback: F) -> ModifiableMemShare<T> where F: Fn() -> T {
        match self.try_pull_from_all_buckets() {
            None => {
                ModifiableMemShare {
                    pool: Arc::clone(self),
                    mem: Some(fallback()),
                }
            }
            Some(mem) => {
                mem
            }
        }
    }

    /// Re attach a given memory into the pool
    #[inline]
    fn re_attach(&self, mem: T) {
        let round_robin = self.insert_ctr.fetch_add(1, Ordering::Relaxed);
        let bucket_len = self.buckets.len();

        let bucket = round_robin % bucket_len;

        // We will speculatively try to insert it into the other buckets as it is important for us to not
        // Waste memory while also not using too many cross thread operations on the insert ctr
        let mut bucket_inc = 0;

        loop {
            if bucket_inc > bucket_len {
                // We have tried too many times, ignore this memory and let it be discarded
                break;
            }

            let final_bucket = (bucket + bucket_inc) % bucket_len;

            let mut bucket_guard = self.buckets[final_bucket].lock().unwrap();

            if bucket_guard.len() >= self.capacity {
                bucket_inc += 1
            } else {
                bucket_guard.push(mem);

                break;
            }
        }

        // If we can't place it anywhere, then forget it :(
    }
}

/// Delimit some common methods of pooled memory
pub trait PooledMem<T> {
    fn detach(self) -> T;
}

pub struct ModifiableMemShare<T> {
    pool: Arc<MemPool<T>>,
    mem: Option<T>,
}

impl<T> ModifiableMemShare<T> {
    pub fn freeze(mut self) -> Arc<ShareableMemShare<T>> {
        Arc::new(ShareableMemShare {
            //TODO: Can we reduce the amount of Arc clones?
            pool: self.pool.clone(),
            mem: self.mem.take(),
        })
    }
}

impl<T> PooledMem<T> for ModifiableMemShare<T> {
    fn detach(mut self) -> (T, Arc<MemPool<T>>) {
        let option = self.mem.take();

        if let Some(mem) = option {
            (mem, self.pool.clone())
        } else {
            unreachable!()
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
        let mem = self.mem.take();

        if let Some(mem) = mem {
            self.pool.re_attach(mem)
        } else {
            //The memory has already been taken
            // This will only happen when we are moving from
            // Modifiable mem shares to shareable mem shares
        }
    }
}

pub struct ShareableMemShare<T> {
    pool: Arc<MemPool<T>>,
    mem: Option<T>,
}

impl<T> ShareableMemShare<T> {
    pub fn unfreeze(mut self) -> ModifiableMemShare<T> {
        ModifiableMemShare {
            pool: self.pool.clone(),
            mem: self.mem.take(),
        }
    }
}

impl<T> PooledMem<T> for ShareableMemShare<T> {
    fn detach(mut self) -> (T, Arc<MemPool<T>>) {
        let option = self.mem.take();

        if let Some(mem) = option {
            (mem, self.pool.clone())
        } else {
            unreachable!()
        }
    }
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

impl<T> Drop for ShareableMemShare<T> {
    fn drop(&mut self) {
        let memory = self.mem.take();

        if let Some(mem) = memory {
            self.pool.re_attach(mem)
        } else {
            //The memory has already been taken
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