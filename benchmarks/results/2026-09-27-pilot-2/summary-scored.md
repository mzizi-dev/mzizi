# mzbench summary

23 finished episode(s); 1 unfinished (counted in no denominator).

Denominators: clean compile over finished episodes; iterations over clean episodes; tokens over episodes with a non-null value; defect rate, mean defects and jaccard over clean episodes that were scored. Token columns are not the same measurement across modes: see each episode's `token_source`.

## By model and arm

| model                          | arm    | n   | clean compile | iters to clean mean | iters to clean median | transcript tokens mean | endpoint tokens mean | defect rate  | clean unscored | mean defects | class-token jaccard mean |
| ------------------------------ | ------ | --- | ------------- | ------------------- | --------------------- | ---------------------- | -------------------- | ------------ | -------------- | ------------ | ------------------------ |
| claude-sonnet-5-subagent       | dioxus | 6   | 6/6 (100.0%)  | 1.00 (n=6)          | 1.0 (n=6)             | 4545 (n=6)             | — (n=0)              | 0/6 (0.0%)   | 0              | 0.00 (n=6)   | 1.000 (n=6)              |
| claude-sonnet-5-subagent       | mzizi  | 6   | 6/6 (100.0%)  | 1.17 (n=6)          | 1.0 (n=6)             | 4172 (n=6)             | — (n=0)              | 0/6 (0.0%)   | 0              | 0.00 (n=6)   | 0.844 (n=6)              |
| qwen2.5-coder-7b-instruct-q4km | dioxus | 6   | 4/6 (66.7%)   | 2.00 (n=4)          | 1.0 (n=4)             | 7782 (n=6)             | 22876 (n=6)          | 0/4 (0.0%)   | 0              | 0.00 (n=4)   | 1.000 (n=4)              |
| qwen2.5-coder-7b-instruct-q4km | mzizi  | 5   | 2/5 (40.0%)   | 1.50 (n=2)          | 1.5 (n=2)             | 8608 (n=5)             | 25615 (n=5)          | 2/2 (100.0%) | 0              | 1.00 (n=2)   | 1.000 (n=2)              |

## By task

| model                          | arm    | task   | n   | clean compile | iters to clean mean | iters to clean median | transcript tokens mean | endpoint tokens mean | defect rate  | clean unscored | mean defects | class-token jaccard mean |
| ------------------------------ | ------ | ------ | --- | ------------- | ------------------- | --------------------- | ---------------------- | -------------------- | ------------ | -------------- | ------------ | ------------------------ |
| claude-sonnet-5-subagent       | dioxus | badge  | 3   | 3/3 (100.0%)  | 1.00 (n=3)          | 1.0 (n=3)             | 4011 (n=3)             | — (n=0)              | 0/3 (0.0%)   | 0              | 0.00 (n=3)   | 1.000 (n=3)              |
| claude-sonnet-5-subagent       | dioxus | button | 3   | 3/3 (100.0%)  | 1.00 (n=3)          | 1.0 (n=3)             | 5078 (n=3)             | — (n=0)              | 0/3 (0.0%)   | 0              | 0.00 (n=3)   | 1.000 (n=3)              |
| claude-sonnet-5-subagent       | mzizi  | badge  | 3   | 3/3 (100.0%)  | 1.00 (n=3)          | 1.0 (n=3)             | 3629 (n=3)             | — (n=0)              | 0/3 (0.0%)   | 0              | 0.00 (n=3)   | 0.796 (n=3)              |
| claude-sonnet-5-subagent       | mzizi  | button | 3   | 3/3 (100.0%)  | 1.33 (n=3)          | 1.0 (n=3)             | 4714 (n=3)             | — (n=0)              | 0/3 (0.0%)   | 0              | 0.00 (n=3)   | 0.891 (n=3)              |
| qwen2.5-coder-7b-instruct-q4km | dioxus | badge  | 3   | 3/3 (100.0%)  | 1.00 (n=3)          | 1.0 (n=3)             | 3931 (n=3)             | 3949 (n=3)           | 0/3 (0.0%)   | 0              | 0.00 (n=3)   | 1.000 (n=3)              |
| qwen2.5-coder-7b-instruct-q4km | dioxus | button | 3   | 1/3 (33.3%)   | 5.00 (n=1)          | 5.0 (n=1)             | 11634 (n=3)            | 41802 (n=3)          | 0/1 (0.0%)   | 0              | 0.00 (n=1)   | 1.000 (n=1)              |
| qwen2.5-coder-7b-instruct-q4km | mzizi  | badge  | 2   | 0/2 (0.0%)    | — (n=0)             | — (n=0)               | 11399 (n=2)            | 38150 (n=2)          | 0/0 (—)      | 0              | — (n=0)      | — (n=0)                  |
| qwen2.5-coder-7b-instruct-q4km | mzizi  | button | 3   | 2/3 (66.7%)   | 1.50 (n=2)          | 1.5 (n=2)             | 6748 (n=3)             | 17258 (n=3)          | 2/2 (100.0%) | 0              | 1.00 (n=2)   | 1.000 (n=2)              |

## Unfinished episodes

- `/tmp/claude-0/-workspace-mzizi/d6ecf655-0263-5808-81b7-f535507e0029/scratchpad/results/scored/qwen2.5-coder-7b-instruct-q4km/mzizi/badge/seed-2`
