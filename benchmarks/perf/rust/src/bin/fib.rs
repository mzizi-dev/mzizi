//! Hand-written Rust reference for `programs/fib.mz` (benchmarks/perf). The same algorithm
//! as the Mzizi program, written as a Rust programmer would. `checked` and `unchecked` build
//! this one file; only the `overflow-checks` setting differs (see ../../Cargo.toml).

fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn run() {
    let n = 38;
    let f = fib(n);
    println!("fib({n}) is {f}");
    println!("checksum {}", f % 1_000_003);
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
