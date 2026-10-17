# Local test runtimes share one stdin/stdout protocol

`rak test` splits local testing into a language-agnostic judge (`src/judge/`: testcase parsing, process running, answer extraction, comparison, verdicts) and per-language Runtimes (`src/judge/runtime/`) that only build. This works because every generated solution harness speaks the same protocol, so a new language only needs a scaffold that emits a conforming `main()` and a Runtime that builds it:

- stdin: one test-case argument per line, as written in `testcases.txt`.
- stdout: anything the user prints, then a final line `output: <value>` with the answer in LeetCode's JSON-like format. The judge takes the last such line, so debug prints before it are kept and shown.
- exit code 2 with `LC_IO: ...` on stderr means the harness could not parse the input (Invalid Input), not a bug in the solution.

Answers are compared as JSON values, with LeetCode's 1e-5 tolerance when either side is a float. Problems that accept any answer order and custom checkers are out of scope. Builds go to the user cache dir (`~/Library/Caches/rak/build` on macOS), not the practice repo, and are reused while the compiler, flags, source and `LC_IO.h` are unchanged.
