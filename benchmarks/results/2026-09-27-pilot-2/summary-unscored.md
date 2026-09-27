# mzbench summary

3 finished episode(s); 1 unfinished (counted in no denominator).

Denominators: clean compile over finished episodes; iterations over clean episodes; tokens over episodes with a non-null value; defect rate, mean defects and jaccard over clean episodes that were scored. Token columns are not the same measurement across modes: see each episode's `token_source`.

## By model and arm

| model                          | arm    | n   | clean compile | iters to clean mean | iters to clean median | transcript tokens mean | endpoint tokens mean | defect rate  | clean unscored | mean defects | class-token jaccard mean |
| ------------------------------ | ------ | --- | ------------- | ------------------- | --------------------- | ---------------------- | -------------------- | ------------ | -------------- | ------------ | ------------------------ |
| claude-sonnet-5-subagent       | dioxus | 1   | 1/1 (100.0%)  | 1.00 (n=1)          | 1.0 (n=1)             | 5801 (n=1)             | — (n=0)              | 1/1 (100.0%) | 0              | 2.00 (n=1)   | — (n=0)                  |
| claude-sonnet-5-subagent       | mzizi  | 1   | 1/1 (100.0%)  | 1.00 (n=1)          | 1.0 (n=1)             | 5055 (n=1)             | — (n=0)              | 1/1 (100.0%) | 0              | 2.00 (n=1)   | — (n=0)                  |
| qwen2.5-coder-7b-instruct-q4km | dioxus | 1   | 0/1 (0.0%)    | — (n=0)             | — (n=0)               | 12903 (n=1)            | 46745 (n=1)          | 0/0 (—)      | 0              | — (n=0)      | — (n=0)                  |

## By task

| model                          | arm    | task                      | n   | clean compile | iters to clean mean | iters to clean median | transcript tokens mean | endpoint tokens mean | defect rate  | clean unscored | mean defects | class-token jaccard mean |
| ------------------------------ | ------ | ------------------------- | --- | ------------- | ------------------- | --------------------- | ---------------------- | -------------------- | ------------ | -------------- | ------------ | ------------------------ |
| claude-sonnet-5-subagent       | dioxus | nyuchi-changelog-renderer | 1   | 1/1 (100.0%)  | 1.00 (n=1)          | 1.0 (n=1)             | 5801 (n=1)             | — (n=0)              | 1/1 (100.0%) | 0              | 2.00 (n=1)   | — (n=0)                  |
| claude-sonnet-5-subagent       | mzizi  | nyuchi-changelog-renderer | 1   | 1/1 (100.0%)  | 1.00 (n=1)          | 1.0 (n=1)             | 5055 (n=1)             | — (n=0)              | 1/1 (100.0%) | 0              | 2.00 (n=1)   | — (n=0)                  |
| qwen2.5-coder-7b-instruct-q4km | dioxus | nyuchi-changelog-renderer | 1   | 0/1 (0.0%)    | — (n=0)             | — (n=0)               | 12903 (n=1)            | 46745 (n=1)          | 0/0 (—)      | 0              | — (n=0)      | — (n=0)                  |

## Unfinished episodes

- `/tmp/claude-0/-workspace-mzizi/d6ecf655-0263-5808-81b7-f535507e0029/scratchpad/results/unscored/qwen2.5-coder-7b-instruct-q4km/mzizi/nyuchi-changelog-renderer/seed-1`
