//! Hand-written Rust reference for `programs/collatz.mz` (benchmarks/perf). The same
//! algorithm as the Mzizi program, recursion included: a loop would measure a different
//! program. `unchecked` and `checked` build this one file; only the `overflow-checks`
//! setting differs (see ../../Cargo.toml).

fn steps(n: i64) -> i64 {
    if n == 1 {
        return 0;
    }
    if n % 2 == 0 {
        return 1 + steps(n / 2);
    }
    1 + steps(3 * n + 1)
}

fn total(lo: i64, hi: i64) -> i64 {
    if lo == hi {
        return steps(lo);
    }
    let mid = (lo + hi) / 2;
    total(lo, mid) + total(mid + 1, hi)
}

fn longest(lo: i64, hi: i64) -> i64 {
    if lo == hi {
        return steps(lo);
    }
    let mid = (lo + hi) / 2;
    longest(lo, mid).max(longest(mid + 1, hi))
}

fn run() {
    let limit = 300_000;
    let sum = total(1, limit);
    let most = longest(1, limit);
    println!("total steps for 1 to {limit}: {sum}");
    println!("most steps for 1 to {limit}: {most}");
    println!("checksum {}", sum % 1_000_003 + most);
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
