//! The probe crate, offline: the probe file format, and every kind of fact judged against
//! an in-process HTTP server that answers with canned bytes. No network, no toolchain.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use mzizi_benchmark_probe::{
    HeaderExpect, Task, agent, judge, load_task, parse_probes, probe_all, send, serve_and_probe,
    verify,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A server that answers each request by its path, forever, on a background thread.
fn fake_server(answer: fn(&str, &str) -> String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let _ = stream.write_all(answer(&method, &path).as_bytes());
        }
    });
    base
}

fn canned(method: &str, path: &str) -> String {
    match (method, path) {
        ("GET", "/json") => {
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Source: fake\r\nContent-Length: 30\r\nConnection: close\r\n\r\n{\"a\":{\"b\":2},\"n\":null,\"s\":\"x\"}"
                .to_string()
        }
        ("GET", "/chunked") => {
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"
                .to_string()
        }
        ("GET", "/redirect") => {
            "HTTP/1.1 308 Permanent Redirect\r\nLocation: /json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_string()
        }
        ("HEAD", _) => {
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 30\r\nConnection: close\r\n\r\n"
                .to_string()
        }
        _ => {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found"
                .to_string()
        }
    }
}

fn task(probes: &str) -> Task {
    Task {
        dir: PathBuf::from("."),
        name: "t".into(),
        probes: parse_probes(probes).unwrap(),
        references: Vec::new(),
    }
}

#[test]
fn b1s_probe_file_parses_into_20_probes_and_59_facts() {
    let t = load_task(&root().join("benchmarks/tasks/b1-routing")).unwrap();
    assert_eq!(t.name, "b1-routing");
    assert_eq!(t.references, ["reference-mzizi", "reference-rust"]);
    assert_eq!(t.probes.len(), 20);
    let facts: usize = t.probes.iter().map(|p| p.expect.facts()).sum();
    assert_eq!(facts, 59);
    for p in &t.probes {
        let n: u32 = p.clause.strip_prefix("B1.").unwrap().parse().unwrap();
        assert!((1..=8).contains(&n), "{}", p.clause);
    }
}

#[test]
fn every_kind_of_fact_is_judged() {
    let base = fake_server(canned);
    let t = task(
        r#"
[[probe]]
id = "json"
clause = "X.1"
request = { method = "GET", path = "/json" }
expect = { status = 200, headers = { "content-type" = "application/json", "x-source" = "fake", "x-none" = false }, json = { "/a/b" = 2, "/s" = "x" }, json_null = ["/n"] }
"#,
    );
    let facts = probe_all(&t, &base);
    assert_eq!(facts.len(), 7);
    assert!(facts.iter().all(|f| f.ok), "{facts:#?}");
}

#[test]
fn a_wrong_value_fails_its_fact_and_says_what_came_back() {
    let base = fake_server(canned);
    let t = task(
        r#"
[[probe]]
id = "json"
clause = "X.1"
request = { method = "GET", path = "/json" }
expect = { status = 201, headers = { "x-source" = "real" }, json = { "/a/b" = "2" } }
"#,
    );
    let facts = probe_all(&t, &base);
    assert!(facts.iter().all(|f| !f.ok), "{facts:#?}");
    assert_eq!(facts[0].got, "200");
    assert_eq!(facts[1].got, "fake");
    // JSON values compare by type: the string "2" is not the number 2.
    assert_eq!(facts[2].got, "2");
}

#[test]
fn redirects_are_not_followed_and_a_404_is_not_an_error() {
    let base = fake_server(canned);
    let a = agent(Duration::from_secs(5));
    let t = task(
        r#"
[[probe]]
id = "r"
clause = "X.1"
request = { method = "GET", path = "/redirect" }
expect = { status = 308, headers = { location = "/json" }, body = "" }

[[probe]]
id = "n"
clause = "X.2"
request = { method = "DELETE", path = "/nowhere" }
expect = { status = 404, body = "not found" }
"#,
    );
    let facts = probe_all(&t, &base);
    assert!(facts.iter().all(|f| f.ok), "{facts:#?}");
    let got = send(&a, &base, Path::new("."), &t.probes[0].request).unwrap();
    assert_eq!(got.status, 308);
}

#[test]
fn a_chunked_body_is_read_whole_and_head_has_none() {
    let base = fake_server(canned);
    let t = task(
        r#"
[[probe]]
id = "c"
clause = "X.1"
request = { method = "GET", path = "/chunked" }
expect = { body = "hello world" }

[[probe]]
id = "h"
clause = "X.2"
request = { method = "HEAD", path = "/json" }
expect = { status = 200, body = "" }
"#,
    );
    let facts = probe_all(&t, &base);
    assert!(facts.iter().all(|f| f.ok), "{facts:#?}");
}

#[test]
fn no_server_fails_every_fact() {
    let port = mzizi_benchmark_probe::free_port().unwrap();
    let t = task(
        r#"
[[probe]]
id = "x"
clause = "X.1"
request = { method = "GET", path = "/" }
expect = { status = 200, body = "" }
"#,
    );
    let facts = probe_all(&t, &format!("http://127.0.0.1:{port}"));
    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|f| !f.ok && f.got == "no response"));
}

#[test]
fn a_server_that_never_listens_is_start_failed_and_fails_every_fact() {
    let t = task(
        r#"
[[probe]]
id = "x"
clause = "X.1"
request = { method = "GET", path = "/" }
expect = { status = 200 }
"#,
    );
    let log = std::env::temp_dir().join(format!("mzprobe-test-{}.log", std::process::id()));
    let argv = vec!["sh".to_string(), "-c".to_string(), "exit 3".to_string()];
    let o = serve_and_probe(&t, &argv, Path::new("."), &log, Duration::from_secs(10)).unwrap();
    assert!(o.start_failed.is_some());
    assert_eq!(o.failed(), 1);
    let _ = std::fs::remove_file(log);
}

#[test]
fn verify_insists_on_two_references() {
    let t = task(
        r#"
[[probe]]
id = "x"
clause = "X.1"
request = { method = "GET", path = "/" }
expect = { status = 200 }
"#,
    );
    let err = verify(&t, Duration::from_secs(1)).unwrap_err();
    assert!(err.contains("two"), "{err}");
}

#[test]
fn the_format_is_strict() {
    let cases = [
        (
            "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = { stauts = 200 }\n",
            "unknown key `stauts`",
        ),
        (
            "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = { headers = { Allow = \"x\" } }\n",
            "lower-case",
        ),
        (
            "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = {}\n",
            "expects nothing",
        ),
        (
            "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"v1\" }\nexpect = { status = 200 }\n",
            "starts with `/`",
        ),
        (
            "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = { status = 200 }\n[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = { status = 200 }\n",
            "declared twice",
        ),
    ];
    for (src, want) in cases {
        let err = parse_probes(src).unwrap_err();
        assert!(err.contains(want), "{want}: {err}");
    }
    let absent = parse_probes(
        "[[probe]]\nid = \"a\"\nclause = \"X\"\nrequest = { method = \"GET\", path = \"/\" }\nexpect = { headers = { allow = false } }\n",
    )
    .unwrap();
    assert_eq!(absent[0].expect.headers[0].1, HeaderExpect::Absent);
    // `judge` on no response at all fails the fact rather than skipping it.
    assert!(!judge(&absent[0], None)[0].ok);
}
