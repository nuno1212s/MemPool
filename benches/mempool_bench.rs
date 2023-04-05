use std::ops::{Add, Div};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use criterion::{BenchmarkId, black_box, Criterion, criterion_group, criterion_main, Throughput};
use object_pool::Pool;
use mem_pool::MemPool;

static KB: usize = 1024;
static MB: usize = 1024 * KB;
static GB: usize = 1024 * MB;
/// The sizes of memory chunks we are going to use for each allocation
static SIZES: &[usize] = &[
    4 * KB, /*
    16 * KB,
    64 * KB,
    128 * KB,
    512 * KB*/
];

static THREAD_COUNT: &[usize] = &[
    1,
    2,
    4,
    8,
    16,
    32
];

#[derive(Clone, Copy)]
enum TestType {
    // Buckets, Capacity per bucket
    MemPool(usize, usize),
    ObjPool(usize),
    Malloc,
}

#[derive(Clone)]
enum Test<T> {
    MemPool(MemPool<T>),
    ObjPool(Arc<Pool<T>>),
    Malloc,
}

fn worker_alloc_pool<T>(barrier: Arc<Barrier>, iterations: u64, size: usize, pool: Test<T>) -> Duration {
    barrier.wait();

    let start_time = Instant::now();

    for _ in 0..iterations {
        match &pool {
            Test::MemPool(mem) => {
                let _ = black_box(mem.try_pull());
            }
            Test::ObjPool(obj) => {
                let _ = black_box(obj.try_pull());
            }
            Test::Malloc => {
                let _ = black_box(Vec::<u8>::with_capacity(size));
            }
        }
    }

    Instant::now().duration_since(start_time)
}

fn setup_test<T, F>(test: TestType, init_fn: F) -> Test<T> where F: Fn() -> T {
    match test {
        TestType::MemPool(buckets, cap) => {
            let pool = MemPool::new(buckets, cap, init_fn);

            Test::MemPool(pool)
        }
        TestType::ObjPool(cap) => {
            let pool = Arc::new(Pool::new(cap, init_fn));

            Test::ObjPool(pool)
        }
        TestType::Malloc => {
            Test::Malloc
        }
    }
}

fn perform_alloc_test(c: &mut Criterion, name: String, test: TestType) {
    let mut alloc_from_pool = c.benchmark_group(name);

    for size in SIZES {
        for thread_count in THREAD_COUNT {
            alloc_from_pool.throughput(Throughput::Bytes(*size as u64));

            let bench_id = BenchmarkId::from_parameter(format!("{} Threads, {} Memory block Size", thread_count, size));

            alloc_from_pool.bench_with_input(bench_id, &(size, thread_count), |b, &(size, thread_count)| {
                let test = setup_test(test, || { Vec::<u8>::with_capacity(*size) });

                b.iter_custom(|iterations| {
                    let mut rxs = Vec::with_capacity(*thread_count);

                    let iterations_per_thread = iterations / (*thread_count as u64);

                    let barrier = Arc::new(Barrier::new(*thread_count));

                    for thread in 0..*thread_count {
                        let (tx, rx) = oneshot::channel();

                        rxs.push(rx);

                        let barrier = barrier.clone();

                        let thread_test = test.clone();

                        std::thread::Builder::new()
                            .name(format!("Worker thread #{}", thread))
                            .spawn(move || {
                                let dur = worker_alloc_pool(barrier, iterations_per_thread,
                                                            *size, thread_test);
                                tx.send(dur).unwrap();
                            }).unwrap();
                    }

                    let mut durations = Vec::with_capacity(*thread_count);

                    // Average out the durations of all of the workers in order to
                    // Obtain a fair number
                    for x in rxs {
                        let duration = x.recv().unwrap();

                        durations.push(duration);
                    }

                    let mut dur = Duration::new(0, 0);

                    for duration in durations {
                        dur = dur.add(duration);
                    }

                    dur.div(*thread_count as u32)
                });
            });
        }
    }

    alloc_from_pool.finish();
}

fn basics(c: &mut Criterion) {
    perform_alloc_test(c, format!("alloc_from_pool"), TestType::MemPool(8, 1000));

    perform_alloc_test(c, format!("alloc_from_obj_pool"), TestType::ObjPool(1000));

    perform_alloc_test(c, format!("alloc_from_OS"), TestType::Malloc);
}

criterion_group!(benches, basics);
criterion_main!(benches);