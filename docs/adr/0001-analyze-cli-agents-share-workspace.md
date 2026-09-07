# Analyze CLI agents run in the caller's workspace

Claude, Gemini, Grok, and Codex analyzers spawn the vendor CLI in the process cwd with no isolation. Isolating Codex alone would make it the odd one out; applying isolation to every CLI Analyzer is tracked in issue #7 instead.
