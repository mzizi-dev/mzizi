//! Backend-task probes (RFC-0009 §2.3).
//!
//! A **probe** is one HTTP request and the facts expected of its response. Every fact is
//! observable at the HTTP boundary, so it is independent of the language that answered it
//! (RFC-0009 BM-3). A task's probes live in `probes.toml`:
//!
//! ```toml
//! [[probe]]
//! id = "options-allow"
//! clause = "B1.3"
//! request = { method = "OPTIONS", path = "/v1/ui" }
//! expect = { status = 204, headers = { allow = "GET, HEAD, OPTIONS" } }
//! ```
//!
//! Each expected status, header, body and JSON value is one fact. A header expected `false`
//! must be absent. `json` compares values at JSON pointers, deep-equal; `json_null` lists
//! pointers whose value must be `null`, which TOML cannot write. An unknown key is an error,
//! so a misspelt expectation cannot be silently skipped.
//!
//! This crate does not start anything the task does not name: [`verify`] runs each
//! reference's own `start.sh`, and [`serve_and_probe`] runs whatever command it is given.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value as Json;

/// One probe, as `probes.toml` states it.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    /// Unique within the task.
    pub id: String,
    /// The reference contract's clause it checks (RFC-0010 §7).
    pub clause: String,
    /// The request.
    pub request: ProbeRequest,
    /// The facts expected of the response.
    pub expect: Expect,
}

/// A probe's request.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeRequest {
    /// The method, as HTTP spells it.
    pub method: String,
    /// The request target: a path, with an optional `?query`, sent as written.
    pub path: String,
    /// Request headers, in order.
    pub headers: Vec<(String, String)>,
    /// A request body read from a fixture, relative to the task directory.
    pub body_file: Option<PathBuf>,
}

/// What a header must be.
#[derive(Clone, Debug, PartialEq)]
pub enum HeaderExpect {
    /// Exactly this value.
    Is(String),
    /// Absent.
    Absent,
}

/// The facts a probe expects. Each field set is one fact, each map entry one fact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Expect {
    /// The status code.
    pub status: Option<u16>,
    /// Headers by lower-case name.
    pub headers: Vec<(String, HeaderExpect)>,
    /// The whole body, as text.
    pub body: Option<String>,
    /// JSON values at JSON pointers.
    pub json: Vec<(String, Json)>,
    /// JSON pointers whose value must be `null`.
    pub json_null: Vec<String>,
}

impl Expect {
    /// How many facts this expectation holds.
    pub fn facts(&self) -> usize {
        usize::from(self.status.is_some())
            + self.headers.len()
            + usize::from(self.body.is_some())
            + self.json.len()
            + self.json_null.len()
    }
}

/// A backend task: its directory, its probes, and its references.
#[derive(Clone, Debug)]
pub struct Task {
    /// The task directory.
    pub dir: PathBuf,
    /// The task's name.
    pub name: String,
    /// Its probes, in file order.
    pub probes: Vec<Probe>,
    /// Reference directories, relative to the task, each with a `start.sh`.
    pub references: Vec<String>,
}

fn table<'a>(v: &'a toml::Value, what: &str) -> Result<&'a toml::Table, String> {
    v.as_table().ok_or(format!("{what} must be a table"))
}

fn string(v: &toml::Value, what: &str) -> Result<String, String> {
    v.as_str()
        .map(str::to_string)
        .ok_or(format!("{what} must be a string"))
}

fn only(t: &toml::Table, allowed: &[&str], what: &str) -> Result<(), String> {
    for k in t.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!(
                "{what} has an unknown key `{k}` (allowed: {})",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn to_json(v: &toml::Value) -> Result<Json, String> {
    Ok(match v {
        toml::Value::String(s) => Json::String(s.clone()),
        toml::Value::Integer(i) => Json::from(*i),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number),
        toml::Value::Boolean(b) => Json::Bool(*b),
        toml::Value::Array(a) => Json::Array(a.iter().map(to_json).collect::<Result<_, _>>()?),
        toml::Value::Table(t) => Json::Object(
            t.iter()
                .map(|(k, v)| Ok((k.clone(), to_json(v)?)))
                .collect::<Result<_, String>>()?,
        ),
        toml::Value::Datetime(d) => Json::String(d.to_string()),
    })
}

/// Parse a `probes.toml`.
pub fn parse_probes(text: &str) -> Result<Vec<Probe>, String> {
    let doc: toml::Table = text.parse().map_err(|e| format!("probes.toml: {e}"))?;
    only(&doc, &["probe"], "probes.toml")?;
    let Some(list) = doc.get("probe") else {
        return Ok(Vec::new());
    };
    let list = list
        .as_array()
        .ok_or("`probe` must be an array of tables, `[[probe]]`")?;
    let mut out = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    for (n, p) in list.iter().enumerate() {
        let what = format!("probe {}", n + 1);
        let p = table(p, &what)?;
        only(p, &["id", "clause", "request", "expect"], &what)?;
        let id = string(p.get("id").ok_or(format!("{what} has no `id`"))?, "id")?;
        if !ids.insert(id.clone()) {
            return Err(format!("probe `{id}` is declared twice"));
        }
        let what = format!("probe `{id}`");
        let clause = string(
            p.get("clause").ok_or(format!("{what} has no `clause`"))?,
            "clause",
        )?;
        let r = table(
            p.get("request").ok_or(format!("{what} has no `request`"))?,
            "request",
        )?;
        only(
            r,
            &["method", "path", "headers", "body_file"],
            &format!("{what} request"),
        )?;
        let method = string(
            r.get("method").ok_or(format!("{what} has no method"))?,
            "method",
        )?;
        let path = string(r.get("path").ok_or(format!("{what} has no path"))?, "path")?;
        if !path.starts_with('/') {
            return Err(format!("{what}: a path starts with `/`, found `{path}`"));
        }
        let mut headers = Vec::new();
        if let Some(h) = r.get("headers") {
            for (k, v) in table(h, "request headers")? {
                headers.push((k.clone(), string(v, "a request header")?));
            }
        }
        let body_file = r
            .get("body_file")
            .map(|v| string(v, "body_file").map(PathBuf::from))
            .transpose()?;
        let e = table(
            p.get("expect").ok_or(format!("{what} has no `expect`"))?,
            "expect",
        )?;
        only(
            e,
            &["status", "headers", "body", "json", "json_null"],
            &format!("{what} expect"),
        )?;
        let mut expect = Expect::default();
        if let Some(s) = e.get("status") {
            let s = s.as_integer().ok_or("status must be an integer")?;
            expect.status = Some(u16::try_from(s).map_err(|_| format!("status {s}"))?);
        }
        if let Some(h) = e.get("headers") {
            for (k, v) in table(h, "expected headers")? {
                if k.to_ascii_lowercase() != *k {
                    return Err(format!(
                        "{what}: expected header names are lower-case, found `{k}`"
                    ));
                }
                let want = match v {
                    toml::Value::Boolean(false) => HeaderExpect::Absent,
                    other => HeaderExpect::Is(string(other, "an expected header")?),
                };
                expect.headers.push((k.clone(), want));
            }
        }
        if let Some(b) = e.get("body") {
            expect.body = Some(string(b, "body")?);
        }
        if let Some(j) = e.get("json") {
            for (ptr, v) in table(j, "json")? {
                if !ptr.is_empty() && !ptr.starts_with('/') {
                    return Err(format!("{what}: `{ptr}` is not a JSON pointer"));
                }
                expect.json.push((ptr.clone(), to_json(v)?));
            }
        }
        if let Some(j) = e.get("json_null") {
            for v in j.as_array().ok_or("json_null must be an array")? {
                expect.json_null.push(string(v, "a JSON pointer")?);
            }
        }
        if expect.facts() == 0 {
            return Err(format!("{what} expects nothing"));
        }
        out.push(Probe {
            id,
            clause,
            request: ProbeRequest {
                method,
                path,
                headers,
                body_file,
            },
            expect,
        });
    }
    Ok(out)
}

/// Load a backend task: `task.toml` (its `name`, and `[backend] references`) and
/// `probes.toml`.
pub fn load_task(dir: &Path) -> Result<Task, String> {
    let text = std::fs::read_to_string(dir.join("task.toml"))
        .map_err(|e| format!("{}: {e}", dir.join("task.toml").display()))?;
    let doc: toml::Table = text.parse().map_err(|e| format!("task.toml: {e}"))?;
    let name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| dir.display().to_string());
    let references = match doc.get("backend").and_then(|b| b.get("references")) {
        Some(r) => r
            .as_array()
            .ok_or("[backend] references must be an array")?
            .iter()
            .map(|v| string(v, "a reference"))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let probes_text = std::fs::read_to_string(dir.join("probes.toml"))
        .map_err(|e| format!("{}: {e}", dir.join("probes.toml").display()))?;
    Ok(Task {
        dir: dir.to_path_buf(),
        name,
        probes: parse_probes(&probes_text)?,
        references,
    })
}

/// A response, as a probe reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Observed {
    /// Status code.
    pub status: u16,
    /// Headers, lower-case names, in order.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

/// One fact's outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    /// The probe it belongs to.
    pub probe: String,
    /// The clause the probe checks.
    pub clause: String,
    /// What was read: `status`, `header allow`, `body`, `json /error`, `json_null /x`.
    pub what: String,
    /// Whether it held.
    pub ok: bool,
    /// What was expected, rendered.
    pub expected: String,
    /// What was observed, rendered.
    pub got: String,
}

/// An HTTP agent that follows no redirect and treats no status as an error: both are
/// facts under test.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(0)
        .allow_non_standard_methods(true)
        .build()
        .into()
}

/// Send one probe's request to `base` (e.g. `http://127.0.0.1:8080`).
pub fn send(
    agent: &ureq::Agent,
    base: &str,
    task_dir: &Path,
    r: &ProbeRequest,
) -> Result<Observed, String> {
    let body = match &r.body_file {
        Some(p) => std::fs::read(task_dir.join(p)).map_err(|e| format!("{}: {e}", p.display()))?,
        None => Vec::new(),
    };
    let mut builder = ureq::http::Request::builder()
        .method(r.method.as_str())
        .uri(format!("{}{}", base.trim_end_matches('/'), r.path));
    for (k, v) in &r.headers {
        builder = builder.header(k, v);
    }
    let req = builder
        .body(body)
        .map_err(|e| format!("{} {}: {e}", r.method, r.path))?;
    let mut res = agent
        .run(req)
        .map_err(|e| format!("{} {}: {e}", r.method, r.path))?;
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let mut body = Vec::new();
    if r.method != "HEAD" {
        res.body_mut()
            .as_reader()
            .read_to_end(&mut body)
            .map_err(|e| format!("{} {}: reading the body: {e}", r.method, r.path))?;
    }
    Ok(Observed {
        status,
        headers,
        body,
    })
}

/// Judge every fact of one probe against what was observed. `None` means the request did
/// not complete, which fails every fact.
pub fn judge(p: &Probe, got: Option<&Observed>) -> Vec<Fact> {
    let mut out = Vec::new();
    let mut fact = |what: String, expected: String, observed: Option<String>, ok: bool| {
        out.push(Fact {
            probe: p.id.clone(),
            clause: p.clause.clone(),
            what,
            ok: ok && observed.is_some(),
            expected,
            got: observed.unwrap_or_else(|| "no response".to_string()),
        });
    };
    let e = &p.expect;
    if let Some(s) = e.status {
        let g = got.map(|o| o.status);
        fact(
            "status".into(),
            s.to_string(),
            g.map(|x| x.to_string()),
            g == Some(s),
        );
    }
    for (name, want) in &e.headers {
        let g = got.map(|o| {
            o.headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        });
        let (expected, ok) = match want {
            HeaderExpect::Is(v) => (
                v.clone(),
                g.as_ref().is_some_and(|x| x.as_deref() == Some(v.as_str())),
            ),
            HeaderExpect::Absent => (
                "absent".to_string(),
                g.as_ref().is_some_and(Option::is_none),
            ),
        };
        let rendered = g.map(|x| x.unwrap_or_else(|| "absent".to_string()));
        fact(format!("header {name}"), expected, rendered, ok);
    }
    if let Some(b) = &e.body {
        let g = got.map(|o| String::from_utf8_lossy(&o.body).into_owned());
        let ok = g.as_deref() == Some(b.as_str());
        fact("body".into(), b.clone(), g, ok);
    }
    let parsed: Option<Result<Json, String>> =
        got.map(|o| serde_json::from_slice::<Json>(&o.body).map_err(|e| format!("not JSON: {e}")));
    for (ptr, want) in &e.json {
        let at = parsed.as_ref().map(|r| match r {
            Ok(v) => v.pointer(ptr).map_or("absent".to_string(), Json::to_string),
            Err(e) => e.clone(),
        });
        let ok = matches!(&parsed, Some(Ok(v)) if v.pointer(ptr) == Some(want));
        fact(format!("json {ptr}"), want.to_string(), at, ok);
    }
    for ptr in &e.json_null {
        let at = parsed.as_ref().map(|r| match r {
            Ok(v) => v.pointer(ptr).map_or("absent".to_string(), Json::to_string),
            Err(e) => e.clone(),
        });
        let ok = matches!(&parsed, Some(Ok(v)) if v.pointer(ptr) == Some(&Json::Null));
        fact(format!("json_null {ptr}"), "null".into(), at, ok);
    }
    out
}

/// Run every probe of `task` against `base`.
pub fn probe_all(task: &Task, base: &str) -> Vec<Fact> {
    let agent = agent(Duration::from_secs(10));
    let mut facts = Vec::new();
    for p in &task.probes {
        let got = send(&agent, base, &task.dir, &p.request).ok();
        facts.extend(judge(p, got.as_ref()));
    }
    facts
}

/// A port nothing is listening on, from the OS.
pub fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no free port: {e}"))?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| format!("no free port: {e}"))
}

/// Wait until `port` accepts a connection, the child exits, or `timeout` passes.
pub fn wait_ready(child: &mut Child, port: u16, timeout: Duration) -> Result<(), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let started = Instant::now();
    while started.elapsed() < timeout {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("the server exited before it listened ({status})"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "nothing listened on port {port} within {timeout:?}"
    ))
}

/// The result of starting a server and probing it.
#[derive(Debug)]
pub struct Outcome {
    /// Every fact, failed ones included.
    pub facts: Vec<Fact>,
    /// Why the server never answered, when it did not (RFC-0009 §2.3's `start_failed`:
    /// every fact then fails).
    pub start_failed: Option<String>,
}

impl Outcome {
    /// Facts that did not hold.
    pub fn failed(&self) -> usize {
        self.facts.iter().filter(|f| !f.ok).count()
    }
}

/// Start `argv` in `dir` with `PORT` set, probe it, and stop it. The command must `exec`
/// the server (or be it), so that stopping the child stops the server. `log` receives its
/// output.
pub fn serve_and_probe(
    task: &Task,
    argv: &[String],
    dir: &Path,
    log: &Path,
    timeout: Duration,
) -> Result<Outcome, String> {
    let (cmd, args) = argv.split_first().ok_or("no command to start")?;
    let port = free_port()?;
    let out = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .env("PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("cannot start {cmd}: {e}"))?;
    let ready = wait_ready(&mut child, port, timeout);
    let outcome = match ready {
        Ok(()) => Outcome {
            facts: probe_all(task, &format!("http://127.0.0.1:{port}")),
            start_failed: None,
        },
        Err(why) => Outcome {
            facts: task.probes.iter().flat_map(|p| judge(p, None)).collect(),
            start_failed: Some(why),
        },
    };
    let _ = child.kill();
    let _ = child.wait();
    Ok(outcome)
}

/// Every reference of `task`, each started with its `start.sh` and probed. RFC-0009 §2.3:
/// every probe must pass against every reference before any run.
pub fn verify(task: &Task, timeout: Duration) -> Result<BTreeMap<String, Outcome>, String> {
    if task.references.len() < 2 {
        return Err(format!(
            "task `{}` names {} reference(s); RFC-0009 §2.3 asks for two, in two languages",
            task.name,
            task.references.len()
        ));
    }
    let mut out = BTreeMap::new();
    for r in &task.references {
        let dir = task.dir.join(r);
        let script = dir.join("start.sh");
        if !script.is_file() {
            return Err(format!("{} does not exist", script.display()));
        }
        let log = std::env::temp_dir().join(format!(
            "mzprobe-{}-{}-{}.log",
            task.name,
            r.replace('/', "_"),
            std::process::id()
        ));
        let argv = vec!["sh".to_string(), "start.sh".to_string()];
        out.insert(
            r.clone(),
            serve_and_probe(task, &argv, &dir, &log, timeout)?,
        );
    }
    Ok(out)
}

/// One fact as an NDJSON line.
pub fn fact_json(f: &Fact) -> String {
    serde_json::json!({
        "probe": f.probe,
        "clause": f.clause,
        "fact": f.what,
        "ok": f.ok,
        "expected": f.expected,
        "got": f.got,
    })
    .to_string()
}
