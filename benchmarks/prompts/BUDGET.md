# Guide budget (RFC-0009 §4.2)

Every guide in a task family is written to one token budget, within ±5%, counted with the
headline model's tokenizer. The `ui` family's budget is **2750 tokens**.

| Arm      | Guide             | Tokens | Against budget | Bytes | SHA-256                                                            |
| -------- | ----------------- | ------ | -------------- | ----- | ------------------------------------------------------------------ |
| `mzizi`  | `mzizi-guide.md`  | 2701   | -1.8%          | 9929  | `2754885bbc85d1e393d0cd6863722782c449f03a0ae8a6e8317669058bd904de` |
| `dioxus` | `dioxus-guide.md` | 2750   | +0.0%          | 11087 | `d0365266c049c9da157d71bffe9184cefb73c7d6ba00dd75d39e07fe7a0d0208` |
| `leptos` | `leptos-guide.md` | 2722   | -1.0%          | 10931 | `2628d31ad3bf59ddd533dec22afe00766f436f065df896d829b4972c1facf6e8` |
| `react`  | `react-guide.md`  | 2691   | -2.1%          | 10298 | `6a9ea65dfdc62a04e7fa0a0138023e085ea920c32cd786d6dc0748f3ee138c4d` |

The largest guide is 2.2% larger than the smallest. Before this rebalancing
(2026-09-29), the three guides were 3,360 (Mzizi), 2,679 (Leptos) and 2,570 (Dioxus) tokens, a
31% spread, and there was no React guide.

The `backend` family has one guide so far, `mzizi-be-guide.md`: 2210 tokens, 7861 bytes,
SHA-256 `2d8b7dac3074e8382175af8bee97b0e97a3f2a7211a33e472aff63b849529486`, counted the same
way on 2026-09-30. It has no budget yet. The budget is set, and this guide rebalanced to it,
when the incumbent backend arms' guides are written ([`../arms/mzizi-be/README.md`](../arms/mzizi-be/README.md)).

**How these were counted.** With the Hugging Face `tokenizers` library (Python) and
`Qwen/Qwen2.5-Coder-7B-Instruct`'s `tokenizer.json` (SHA-256
`c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539`), on 2026-09-30. That is
the headline model's tokenizer, but not the same code path as llama.cpp's `/tokenize`, which a
run uses. Each episode's final line records `guide_tokens` from the run's own endpoint, and
the run's `PLAN.md` states those counts. If they disagree with this table, the run's counts are
the fact.

**What keeps this table honest.** `cargo test -p mzizi-benchmark-runner` checks every UI arm's
guide against its SHA-256 here, and checks that every count is within ±5% of the budget. A
guide edited without being re-measured fails that test. Re-measure with the tokenizer above
and update the row in the same commit.

**What every guide contains**, in this order: file shape, what is available, naming rules,
reading the checker's output (a file wrong on purpose, and the checker's real output for it),
and one worked example from the corpus that is not a task: the registry's avatar.
`verify-guide.sh <guide>` runs every block through that arm's real checker, and checks that
each wrong-on-purpose block's output matches the guide byte for byte.
