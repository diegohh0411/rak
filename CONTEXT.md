# RAK

A CLI for internship and LeetCode practice: record voice notes on a problem, transcribe them, produce an analysis of the attempt, and schedule reviews with a Leitner system.

## Language

**Analyzer**:
A registered backend that turns a problem's question, latest solution, and transcripts into an Analysis with a single completion.
_Avoid_: agent, harness, ADK

**Analysis**:
The markdown critique of one problem attempt, stored as `analysis.md` and later stitched into `analyses-master.md`.
_Avoid_: report, review

**Provider**:
A named backend for transcribe or analyze, chosen in `rak.toml` and optionally overridden on the CLI.

**Runtime**:
A registered language backend for `rak test` that builds a problem's solution file into an Executable, e.g. the C++ runtime compiling `solution.cpp`. It only builds; running and judging are shared.
_Avoid_: compiler, toolchain, language provider

**Verdict**:
The judged outcome of one test case: Accepted, Wrong Answer, Runtime Error, Time Limit Exceeded, Invalid Input, or No expected output.
_Avoid_: result, status

**Workspace** (TUI):
The one interactive screen `rak` opens: a sidebar listing problems, and for the selected problem six tabs (Question, Tests, Record, Analysis, Submit, History). Every interactive command opens it on the matching tab.
_Avoid_: dashboard, app

**Job**:
Work the TUI runs on a background thread so the screen stays live: a test run, the transcribe-then-analyze pipeline after a recording, a submission, a LeetCode fetch. Jobs report back with a message; they never print.
_Avoid_: task, worker
