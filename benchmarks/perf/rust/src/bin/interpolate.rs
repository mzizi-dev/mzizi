//! Hand-written Rust reference for `programs/interpolate.mz` (benchmarks/perf). The same
//! algorithm as the Mzizi program: `format!` where Mzizi interpolates, `==` on `String`
//! where it writes `is`. `unchecked` and `checked` build this one file; only the
//! `overflow-checks` setting differs (see ../../Cargo.toml).

fn label(i: i64, n: i64) -> String {
    format!(
        "item {i} of {n}: square {}, cube {}, odd {}",
        i * i,
        i * i * i,
        i % 2 == 1
    )
}

fn nest(i: i64, depth: i64) -> String {
    if depth == 0 {
        return format!("<{i}>");
    }
    format!("[{depth}:{}]", nest(i, depth - 1))
}

fn score(lo: i64, hi: i64, n: i64) -> i64 {
    if lo == hi {
        let built = format!("{} {}", label(lo, n), nest(lo, 3));
        let expected = format!(
            "item {lo} of {n}: square {}, cube {}, odd {} [3:[2:[1:<{lo}>]]]",
            lo * lo,
            lo * lo * lo,
            lo % 2 == 1
        );
        if built == expected {
            return lo % 7 + 1;
        }
        return 0;
    }
    let mid = (lo + hi) / 2;
    score(lo, mid, n) + score(mid + 1, hi, n)
}

fn run() {
    let n = 200_000;
    println!("{}", label(7, n));
    println!("{}", nest(3, 4));
    println!("checksum {}", score(1, n, n));
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
