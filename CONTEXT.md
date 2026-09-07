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
