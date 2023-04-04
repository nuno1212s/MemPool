use std::ops::{Add, Div};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, Throughput};
use mem_pool::MemPool;

static KB: usize = 1024;
static MB: usize = 1024 * KB;
static GB: usize = 1024 * MB;
/// The sizes of memory chunks we are going to use for each allocation
static SIZES: &[usize] = &[
    4 * KB,
    16 * KB,
    64 * KB,
    128 * KB,
    512 * KB
];

static THREAD_COUNT: &[usize] = &[
    1,
    2,
    4,
    8,
    16,
    32
];

fn perform_worker_test<F, T>(iterations: u64, worker: F) where F: Fn() -> T {

}

fn perform_alloc_test<F, T>(c: &mut Criterion, name: String, func: F) where F: Fn() -> T {
    let mut alloc_from_pool = c.benchmark_group(name);

    for size in SIZES {
        for thread_count in THREAD_COUNT {
            alloc_from_pool.throughput(Throughput::Bytes(*size as u64));

            let bench_id = BenchmarkId::from_parameter(format!("{} Threads, {} Memory block Size", thread_count, size));

            alloc_from_pool.bench_with_input(bench_id, &(size, thread_count), |b, &(size, thread_count)| {
                let pool = MemPool::new(8, 1000, || { Vec::<u8>::with_capacity(*size) });

                b.iter_custom(|iterations| {
                    let mut rxs = Vec::with_capacity(*thread_count);

                    let iterations_per_thread = iterations / (*thread_count as u64);

                    let barrier = Arc::new(Barrier::new(*thread_count));

                    for thread in 0..*thread_count {
                        let (tx, rx) = oneshot::channel();

                        rxs.push(rx);

                        let pool_ref = pool.clone();
                        let barrier = barrier.clone();

                        std::thread::Builder::new()
                            .name(format!("Worker thread #{}", thread))
                            .spawn(move || {

                                barrier.wait();

                                let start_time = Instant::now();

                                for _ in 0..iterations_per_thread {
                                    let _ = pool_ref.try_pull();
                                }

                                let dur = Instant::now().duration_since(start_time);

                                tx.send(dur).unwrap();
                            }).unwrap();
                    }

                    let mut durations = Vec::with_capacity(*thread_count);

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
    let mut alloc_from_pool = c.benchmark_group("alloc_from_pool");

    for size in SIZES {
        for thread_count in THREAD_COUNT {
            alloc_from_pool.throughput(Throughput::Bytes(*size as u64));

            let bench_id = BenchmarkId::from_parameter(format!("{} Threads, {} Memory block Size", thread_count, size));

            alloc_from_pool.bench_with_input(bench_id, &(size, thread_count), |b, &(size, thread_count)| {
                let pool = MemPool::new(8, 1000, || { Vec::<u8>::with_capacity(*size) });

                b.iter_custom(|iterations| {
                    let mut rxs = Vec::with_capacity(*thread_count);

                    let iterations_per_thread = iterations / (*thread_count as u64);

                    let barrier = Arc::new(Barrier::new(*thread_count));

                    for thread in 0..*thread_count {
                        let (tx, rx) = oneshot::channel();

                        rxs.push(rx);

                        let pool_ref = pool.clone();
                        let barrier = barrier.clone();

                        std::thread::Builder::new()
                            .name(format!("Worker thread #{}", thread))
                            .spawn(move || {

                                barrier.wait();

                                let start_time = Instant::now();

                                for _ in 0..iterations_per_thread {
                                    let _ = pool_ref.try_pull();
                                }

                                let dur = Instant::now().duration_since(start_time);

                                tx.send(dur).unwrap();
                            }).unwrap();
                    }

                    let mut durations = Vec::with_capacity(*thread_count);

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

criterion_group!(benches, basics);
criterion_main!(benches);