# Guide budget (RFC-0009 §4.2)

Every guide in a task family is written to one token budget, within ±5%, counted with the
headline model's tokenizer. The `ui` family's budget is **2750 tokens**.

| Arm      | Guide             | Tokens | Against budget | Bytes | SHA-256                                                            |
| -------- | ----------------- | ------ | -------------- | ----- | ------------------------------------------------------------------ |
| `mzizi`  | `mzizi-guide.md`  | 2695   | -2.0%          | 9900  | `58b5632cb648b21913084d64201483eb9d18a011ec997f811c74107ab8860a32` |
| `dioxus` | `dioxus-guide.md` | 2750   | +0.0%          | 11087 | `d0365266c049c9da157d71bffe9184cefb73c7d6ba00dd75d39e07fe7a0d0208` |
| `leptos` | `leptos-guide.md` | 2722   | -1.0%          | 10931 | `2628d31ad3bf59ddd533dec22afe00766f436f065df896d829b4972c1facf6e8` |
| `react`  | `react-guide.md`  | 2691   | -2.1%          | 10298 | `25de0e58048284151db25d26f18768ebfa97f7697bb6bdb21187f8ffd171dfb7` |

The largest guide is 2.2% larger than the smallest. Before this rebalancing
(2026-09-29), the three guides were 3,360 (Mzizi), 2,679 (Leptos) and 2,570 (Dioxus) tokens, a
31% spread, and there was no React guide.

## The `backend` family

The `backend` family's budget is **2210 tokens**: the length of `mzizi-be-guide.md`, which was
written first and is left as it was (counted on 2026-09-30, SHA-256 unchanged since). The five
incumbent guides (TypeScript with Hono, Python with FastAPI, Go with `net/http`, C++20 with
cpp-httplib, Rust with axum) were written and trimmed or extended to it on 2026-10-29. Each
carries the five sections listed below, and no sentence in any of them describes a task: their
worked example is a weather-station service, the same one in every language, and not B1.

| Arm        | Guide               | Tokens | Against budget | Bytes | SHA-256                                                            |
| ---------- | ------------------- | ------ | -------------- | ----- | ------------------------------------------------------------------ |
| `mzizi-be` | `mzizi-be-guide.md` | 2210   | +0.0%          | 7861  | `2d8b7dac3074e8382175af8bee97b0e97a3f2a7211a33e472aff63b849529486` |
| `ts`       | `ts-guide.md`       | 2193   | -0.8%          | 8008  | `334fd7ac370e4b951f671b1a7747f9d6c2cabd486725a1748868e509ffcd4160` |
| `python`   | `python-guide.md`   | 2160   | -2.3%          | 8364  | `b0fabc39705553a1cd4b675c2658e77ab8f388b852ab44805d2662fae3939de0` |
| `go`       | `go-guide.md`       | 2208   | -0.1%          | 8209  | `c1beb893c18860e83744418ff9c93d971da08ee1beb86e213d8a68e3995ac170` |
| `cpp`      | `cpp-guide.md`      | 2202   | -0.4%          | 7924  | `da5985bc90f96f613fa43928ad29c2f6630966905d2dff39b05afb008a75036e` |
| `rust`     | `rust-guide.md`     | 2154   | -2.5%          | 8012  | `f92c1d63dbd5c74d09af6e8b58dd9084adc9a6cd2733be9d9d5623ab6920ad6a` |

The largest backend guide is 2.6% larger than the smallest, counted with the tokenizer below.
Bytes differ more than tokens do (7,861 to 8,364) because the languages tokenise differently;
the budget is in tokens, as RFC-0009 §4.2 says. Each guide's blocks were run through its arm's
real checker with `verify-guide.sh` (the wrong-on-purpose block's output is the checker's, byte
for byte; the worked example checks clean). Nothing here says one guide teaches its language
better than another: equal length is the only claim.

**How these were counted.** With the Hugging Face `tokenizers` library (Python) and
`Qwen/Qwen2.5-Coder-7B-Instruct`'s `tokenizer.json` (SHA-256
`c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539`), on 2026-09-30; the Mzizi
guide was re-measured the same way on 2026-10-07, after its `match` sentence changed (#54). That is
the headline model's tokenizer, but not the same code path as llama.cpp's `/tokenize`, which a
run uses. Each episode's final line records `guide_tokens` from the run's own endpoint, and
the run's `PLAN.md` states those counts. If they disagree with this table, the run's counts are
the fact.

**What keeps these tables honest.** `cargo test -p mzizi-benchmark-runner` checks every arm's
guide against its SHA-256 here, and checks that every count is within ±5% of its family's budget. A
guide edited without being re-measured fails that test. Re-measure with the tokenizer above
and update the row in the same commit.

**What every guide contains**, in this order: file shape, what is available, naming rules,
reading the checker's output (a file wrong on purpose, and the checker's real output for it),
and one worked example from the reference implementations that is not a task: the registry's avatar.
`verify-guide.sh <guide>` runs every block through that arm's real checker, and checks that
each wrong-on-purpose block's output matches the guide byte for byte.
