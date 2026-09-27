//! Phase 0 benchmark runner (`mzbench`).
//!
//! Three modes share one prompt builder, one feedback builder, one iteration recorder and
//! one scorer invocation, so the model-driven arm (llama.cpp over HTTP) and the
//! agent-driven arm (Claude Code subagents submitting files) differ only where they are
//! forced to — how a reply is obtained and where token counts come from — and those
//! differences are written into every episode's output rather than left implicit.
//!
//! - [`prompt`] — the system/user text and the error-feedback text, identical across arms
//!   except for the language name and file kind.
//! - [`extract`] — the one fenced code block in a model reply.
//! - [`task`] — a task directory (`task.toml`, `spec.tsx`, `reference.rs`).
//! - [`exec`] — injectable command templates for the compile check and the scorer.
//! - [`endpoint`] — the llama.cpp client, behind traits so tests need no server.
//! - [`episode`] — the episode state machine shared by `run` and `episode …`.
//! - [`summary`] — `summarize`'s aggregation and markdown rendering.

pub mod endpoint;
pub mod episode;
pub mod exec;
pub mod extract;
pub mod prompt;
pub mod summary;
pub mod task;
