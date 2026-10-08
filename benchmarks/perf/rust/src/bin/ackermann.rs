//! Hand-written Rust reference for `programs/ackermann.mz` (benchmarks/perf). The same
//! algorithm as the Mzizi program. `unchecked` and `checked` build this one file; only the
//! `overflow-checks` setting differs (see ../../Cargo.toml).

fn ack(m: i64, n: i64) -> i64 {
    if m == 0 {
        return n + 1;
    }
    if n == 0 {
        return ack(m - 1, 1);
    }
    ack(m - 1, ack(m, n - 1))
}

fn ack_sum(n: i64) -> i64 {
    if n < 0 {
        return 0;
    }
    ack(3, n) + ack_sum(n - 1)
}

fn run() {
    println!("ack(2, 3) is {}", ack(2, 3));
    println!("ack(3, 10) is {}", ack(3, 10));
    println!("checksum {}", ack_sum(10));
}

/// The same 64 MiB main thread the lowered Mzizi program runs on, so stack room is equal.
fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("spawn the main thread")
        .join()
        .expect("the main thread finished");
}
