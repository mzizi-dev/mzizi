//! `mzbench summarize <results dir>`: aggregate every episode's final line.
//!
//! Every rate and mean is printed with its denominator, so a rate over n=2 reads as one.
//! Denominators, precisely:
//!
//! - clean-compile rate: clean / finished episodes.
//! - iterations to clean: over clean episodes.
//! - transcript / endpoint tokens: over episodes where the value is non-null.
//! - defect rate: clean episodes with ≥1 defect / clean episodes that were **scored**.
//!   A clean episode whose scorer failed is neither defective nor defect-free; it is
//!   excluded and counted in its own column, never folded into "0 defects".
//! - mean defects, class_token_jaccard: over scored clean episodes (non-null jaccard).
//! - renamed variants: a total, over scored clean episodes whose final line carries
//!   `renames` (episodes scored before the harness reported it are not in `n`).
//!
//! Episode directories with no final line (aborted, or agent episodes never finished)
//! are listed separately and counted in no denominator.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeFinal {
    pub model: String,
    pub arm: String,
    pub task: String,
    pub clean: bool,
    pub iterations_to_clean: Option<u64>,
    pub transcript_tokens: Option<u64>,
    pub endpoint_tokens: Option<u64>,
    pub scored: bool,
    pub defects: Option<u64>,
    pub class_token_jaccard: Option<f64>,
    pub renames: Option<u64>,
}

impl EpisodeFinal {
    pub fn from_final_line(v: &Value) -> Option<EpisodeFinal> {
        let s = |k: &str| v[k].as_str().map(str::to_string);
        let endpoint_tokens = match (
            v["total_prompt_tokens"].as_u64(),
            v["total_completion_tokens"].as_u64(),
        ) {
            (Some(p), Some(c)) => Some(p + c),
            _ => None,
        };
        let defects = v["defects"].as_u64();
        Some(EpisodeFinal {
            model: s("model")?,
            arm: s("arm")?,
            task: s("task")?,
            clean: v["clean"].as_bool()?,
            iterations_to_clean: v["iterations_to_clean"].as_u64(),
            transcript_tokens: v["transcript_tokens"].as_u64(),
            endpoint_tokens,
            scored: v["scored"].as_bool().unwrap_or(defects.is_some()) && defects.is_some(),
            defects,
            class_token_jaccard: v["class_token_jaccard"].as_f64(),
            renames: v["renames"].as_u64(),
        })
    }
}

/// Walk `root` for `episode.jsonl` files. Returns finished episodes and the directories
/// of unfinished ones.
pub fn collect(root: &Path) -> Result<(Vec<EpisodeFinal>, Vec<PathBuf>), String> {
    let mut finals = Vec::new();
    let mut unfinished = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "episode.jsonl") {
                let text = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let fin = text
                    .lines()
                    .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                    .find(|v| v["kind"] == "final")
                    .and_then(|v| EpisodeFinal::from_final_line(&v));
                match fin {
                    Some(f) => finals.push(f),
                    None => unfinished.push(d.clone()),
                }
            }
        }
    }
    unfinished.sort();
    Ok((finals, unfinished))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub clean: usize,
    pub iters_mean: Option<f64>,
    pub iters_median: Option<f64>,
    pub iters_n: usize,
    pub transcript_mean: Option<f64>,
    pub transcript_n: usize,
    pub endpoint_mean: Option<f64>,
    pub endpoint_n: usize,
    pub scored: usize,
    pub clean_unscored: usize,
    pub defective: usize,
    pub defects_mean: Option<f64>,
    pub jaccard_mean: Option<f64>,
    pub jaccard_n: usize,
    pub renames_total: u64,
    pub renames_n: usize,
}

impl Stats {
    pub fn defect_rate(&self) -> Option<f64> {
        (self.scored > 0).then(|| self.defective as f64 / self.scored as f64)
    }
    pub fn clean_rate(&self) -> Option<f64> {
        (self.n > 0).then(|| self.clean as f64 / self.n as f64)
    }
}

fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| xs.iter().sum::<f64>() / xs.len() as f64)
}

fn median(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m]
    } else {
        (v[m - 1] + v[m]) / 2.0
    })
}

pub fn compute(eps: &[&EpisodeFinal]) -> Stats {
    let clean: Vec<&&EpisodeFinal> = eps.iter().filter(|e| e.clean).collect();
    let iters: Vec<f64> = clean
        .iter()
        .filter_map(|e| e.iterations_to_clean)
        .map(|x| x as f64)
        .collect();
    let transcript: Vec<f64> = eps
        .iter()
        .filter_map(|e| e.transcript_tokens)
        .map(|x| x as f64)
        .collect();
    let endpoint: Vec<f64> = eps
        .iter()
        .filter_map(|e| e.endpoint_tokens)
        .map(|x| x as f64)
        .collect();
    let scored: Vec<&&&EpisodeFinal> = clean.iter().filter(|e| e.scored).collect();
    let defects: Vec<f64> = scored
        .iter()
        .filter_map(|e| e.defects)
        .map(|x| x as f64)
        .collect();
    let jacc: Vec<f64> = scored
        .iter()
        .filter_map(|e| e.class_token_jaccard)
        .collect();
    let renames: Vec<u64> = scored.iter().filter_map(|e| e.renames).collect();
    Stats {
        n: eps.len(),
        clean: clean.len(),
        iters_mean: mean(&iters),
        iters_median: median(&iters),
        iters_n: iters.len(),
        transcript_mean: mean(&transcript),
        transcript_n: transcript.len(),
        endpoint_mean: mean(&endpoint),
        endpoint_n: endpoint.len(),
        scored: scored.len(),
        clean_unscored: clean.len() - scored.len(),
        defective: defects.iter().filter(|&&d| d >= 1.0).count(),
        defects_mean: mean(&defects),
        jaccard_mean: mean(&jacc),
        jaccard_n: jacc.len(),
        renames_total: renames.iter().sum(),
        renames_n: renames.len(),
    }
}

fn pct(num: usize, den: usize) -> String {
    if den == 0 {
        format!("{num}/0 (—)")
    } else {
        format!("{num}/{den} ({:.1}%)", 100.0 * num as f64 / den as f64)
    }
}

fn num(x: Option<f64>, n: usize, prec: usize) -> String {
    match x {
        Some(v) => format!("{v:.prec$} (n={n})"),
        None => format!("— (n={n})"),
    }
}

const HEADER: &str = "| n | clean compile | iters to clean mean | iters to clean median | \
                      transcript tokens mean | endpoint tokens mean | defect rate | \
                      clean unscored | mean defects | class-token jaccard mean | \
                      renamed variants |";

fn cells(s: &Stats) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} (n={}) |",
        s.n,
        pct(s.clean, s.n),
        num(s.iters_mean, s.iters_n, 2),
        num(s.iters_median, s.iters_n, 1),
        num(s.transcript_mean, s.transcript_n, 0),
        num(s.endpoint_mean, s.endpoint_n, 0),
        pct(s.defective, s.scored),
        s.clean_unscored,
        num(s.defects_mean, s.scored, 2),
        num(s.jaccard_mean, s.jaccard_n, 3),
        s.renames_total,
        s.renames_n,
    )
}

fn separator(cols: usize) -> String {
    format!("|{}", " --- |".repeat(cols))
}

pub fn render(finals: &[EpisodeFinal], unfinished: &[PathBuf]) -> String {
    let mut out = String::new();
    out.push_str("# mzbench summary\n\n");
    out.push_str(&format!(
        "{} finished episode(s); {} unfinished (counted in no denominator).\n\n",
        finals.len(),
        unfinished.len()
    ));
    out.push_str(
        "Denominators: clean compile over finished episodes; iterations over clean \
         episodes; tokens over episodes with a non-null value; defect rate, mean defects \
         and jaccard over clean episodes that were scored. Token columns are not the same \
         measurement across modes: see each episode's `token_source`.\n\n",
    );

    let mut by_arm: BTreeMap<(&str, &str), Vec<&EpisodeFinal>> = BTreeMap::new();
    let mut by_task: BTreeMap<(&str, &str, &str), Vec<&EpisodeFinal>> = BTreeMap::new();
    for f in finals {
        by_arm.entry((&f.model, &f.arm)).or_default().push(f);
        by_task
            .entry((&f.model, &f.arm, &f.task))
            .or_default()
            .push(f);
    }

    out.push_str("## By model and arm\n\n");
    out.push_str(&format!("| model | arm {HEADER}\n{}\n", separator(13)));
    for ((model, arm), eps) in &by_arm {
        out.push_str(&format!("| {model} | {arm} {}\n", cells(&compute(eps))));
    }

    out.push_str("\n## By task\n\n");
    out.push_str(&format!(
        "| model | arm | task {HEADER}\n{}\n",
        separator(14)
    ));
    for ((model, arm, task), eps) in &by_task {
        out.push_str(&format!(
            "| {model} | {arm} | {task} {}\n",
            cells(&compute(eps))
        ));
    }

    if !unfinished.is_empty() {
        out.push_str("\n## Unfinished episodes\n\n");
        for d in unfinished {
            out.push_str(&format!("- `{}`\n", d.display()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(clean: bool, iters: Option<u64>, defects: Option<u64>) -> EpisodeFinal {
        EpisodeFinal {
            model: "m".into(),
            arm: "mzizi".into(),
            task: "t".into(),
            clean,
            iterations_to_clean: iters,
            transcript_tokens: Some(100),
            endpoint_tokens: None,
            scored: defects.is_some(),
            defects,
            class_token_jaccard: defects.map(|_| 0.5),
            renames: defects.map(|_| 1),
        }
    }

    #[test]
    fn rates_means_and_medians() {
        let e = [
            ep(true, Some(1), Some(0)),
            ep(true, Some(2), Some(2)),
            ep(true, Some(4), Some(1)),
            ep(false, None, None),
        ];
        let refs: Vec<&EpisodeFinal> = e.iter().collect();
        let s = compute(&refs);
        assert_eq!(s.n, 4);
        assert_eq!(s.clean, 3);
        assert_eq!(s.clean_rate(), Some(0.75));
        assert_eq!(s.iters_mean, Some(7.0 / 3.0));
        assert_eq!(s.iters_median, Some(2.0));
        assert_eq!(s.defective, 2);
        assert_eq!(s.defect_rate(), Some(2.0 / 3.0));
        assert_eq!(s.defects_mean, Some(1.0));
        assert_eq!(s.transcript_mean, Some(100.0));
        assert_eq!(s.transcript_n, 4);
        assert_eq!(s.endpoint_mean, None);
        assert_eq!(s.endpoint_n, 0);
        assert_eq!(s.jaccard_mean, Some(0.5));
        assert_eq!((s.renames_total, s.renames_n), (3, 3));
        assert!(
            cells(&s).ends_with("| 0.500 (n=3) | 3 (n=3) |"),
            "{}",
            cells(&s)
        );
    }

    #[test]
    fn even_count_median_is_midpoint() {
        assert_eq!(median(&[1.0, 2.0, 4.0, 5.0]), Some(3.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn zero_clean_episodes_has_no_rates_and_no_nan() {
        let e = [ep(false, None, None), ep(false, None, None)];
        let refs: Vec<&EpisodeFinal> = e.iter().collect();
        let s = compute(&refs);
        assert_eq!(s.clean, 0);
        assert_eq!(s.clean_rate(), Some(0.0));
        assert_eq!(s.iters_mean, None);
        assert_eq!(s.iters_median, None);
        assert_eq!(s.defect_rate(), None);
        assert_eq!(s.defects_mean, None);
        let row = cells(&s);
        assert!(row.contains("0/2 (0.0%)"), "{row}");
        assert!(row.contains("0/0 (—)"), "{row}");
        assert!(!row.contains("NaN"), "{row}");
        assert_eq!(compute(&[]).clean_rate(), None);
    }

    #[test]
    fn clean_but_unscored_is_not_counted_as_defect_free() {
        let e = [ep(true, Some(1), None), ep(true, Some(1), Some(1))];
        let refs: Vec<&EpisodeFinal> = e.iter().collect();
        let s = compute(&refs);
        assert_eq!(s.scored, 1);
        assert_eq!(s.clean_unscored, 1);
        assert_eq!(s.defect_rate(), Some(1.0));
    }

    #[test]
    fn render_shows_denominators_and_per_task() {
        let mut a = ep(true, Some(2), Some(0));
        a.task = "button".into();
        let mut b = ep(false, None, None);
        b.task = "card".into();
        let text = render(&[a, b], &[PathBuf::from("/r/x")]);
        assert!(
            text.contains("| m | mzizi | 2 | 1/2 (50.0%) | 2.00 (n=1)"),
            "{text}"
        );
        assert!(
            text.contains("| m | mzizi | button | 1 | 1/1 (100.0%)"),
            "{text}"
        );
        assert!(
            text.contains("| m | mzizi | card | 1 | 0/1 (0.0%)"),
            "{text}"
        );
        assert!(text.contains("1 unfinished"));
        assert!(text.contains("`/r/x`"));
    }

    #[test]
    fn parses_final_line() {
        let v: Value = serde_json::from_str(
            r#"{"kind":"final","model":"m","arm":"dioxus","task":"t","clean":true,
                "iterations_to_clean":2,"total_prompt_tokens":10,"total_completion_tokens":5,
                "transcript_tokens":null,"scored":true,"defects":0,"class_token_jaccard":null}"#,
        )
        .unwrap();
        let f = EpisodeFinal::from_final_line(&v).unwrap();
        assert_eq!(f.endpoint_tokens, Some(15));
        assert_eq!(f.transcript_tokens, None);
        assert!(f.scored);
        assert_eq!(f.renames, None, "a line from before the field existed");
    }
}
