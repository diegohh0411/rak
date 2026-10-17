//! Running a built solution on one test case: feed stdin, capture output,
//! and kill it when it exceeds the time limit.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::runtime::Executable;
use super::verdict::{Execution, Termination};

/// Output past this many bytes per stream is dropped, so a runaway print loop
/// can't eat memory before the time limit kills it.
const MAX_CAPTURE: u64 = 4 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Run `exe` in `cwd` with `stdin` as input, killing it after `timeout`.
pub fn run(
    exe: &Executable,
    cwd: &Path,
    stdin: &str,
    timeout: Duration,
) -> Result<(Execution, Duration), String> {
    let start = Instant::now();
    let mut child = Command::new(&exe.program)
        .args(&exe.args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", exe.program.display()))?;

    // Feed stdin and drain stdout/stderr on their own threads so a program
    // that blocks on a full pipe can't deadlock us.
    let input = stdin.as_bytes().to_vec();
    let mut child_stdin = child.stdin.take().ok_or("failed to open stdin")?;
    let writer = thread::spawn(move || {
        // The program may exit without reading all of its input.
        let _ = child_stdin.write_all(&input);
    });
    let stdout = capture(child.stdout.take().ok_or("failed to open stdout")?);
    let stderr = capture(child.stderr.take().ok_or("failed to open stderr")?);

    let status = wait_with_timeout(&mut child, timeout)?;
    let elapsed = start.elapsed();
    let _ = writer.join();
    let stdout = stdout.join().map_err(|_| "stdout reader panicked")?;
    let stderr = stderr.join().map_err(|_| "stderr reader panicked")?;

    let termination = match status {
        None => Termination::TimedOut,
        Some(status) => termination_of(status),
    };
    Ok((
        Execution {
            stdout,
            stderr,
            termination,
        },
        elapsed,
    ))
}

fn capture(stream: impl Read + Send + 'static) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut limited = stream.take(MAX_CAPTURE);
        let _ = limited.read_to_end(&mut buf);
        // Keep draining so the child never blocks on a full pipe.
        let _ = std::io::copy(&mut limited.into_inner(), &mut std::io::sink());
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// `Ok(None)` means the child was killed for exceeding `timeout`.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().map_err(|e| e.to_string())?;
            return Ok(None);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn termination_of(status: ExitStatus) -> Termination {
    use std::os::unix::process::ExitStatusExt;
    match status.code() {
        Some(code) => Termination::Exited(code),
        None => Termination::Signaled(status.signal()),
    }
}

#[cfg(not(unix))]
fn termination_of(status: ExitStatus) -> Termination {
    match status.code() {
        Some(code) => Termination::Exited(code),
        None => Termination::Signaled(None),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Executable {
        Executable {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
        }
    }

    fn run_sh(script: &str, stdin: &str, timeout_ms: u64) -> Execution {
        let dir = std::env::temp_dir();
        run(&sh(script), &dir, stdin, Duration::from_millis(timeout_ms))
            .unwrap()
            .0
    }

    #[test]
    fn captures_stdin_stdout_stderr() {
        let e = run_sh("cat; echo err >&2", "hello\n", 5000);
        assert_eq!(e.stdout, "hello\n");
        assert_eq!(e.stderr, "err\n");
        assert_eq!(e.termination, Termination::Exited(0));
    }

    #[test]
    fn reports_exit_code() {
        assert_eq!(
            run_sh("exit 3", "", 5000).termination,
            Termination::Exited(3)
        );
    }

    #[test]
    fn kills_on_timeout() {
        let start = Instant::now();
        let e = run_sh("exec sleep 10", "", 200);
        assert_eq!(e.termination, Termination::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn reports_signal() {
        assert_eq!(
            run_sh("kill -SEGV $$", "", 5000).termination,
            Termination::Signaled(Some(11))
        );
    }

    #[test]
    fn ignores_unread_stdin() {
        let big = "x".repeat(1 << 20);
        let e = run_sh("echo done", &big, 5000);
        assert_eq!(e.stdout, "done\n");
        assert_eq!(e.termination, Termination::Exited(0));
    }

    #[test]
    fn missing_program_errors() {
        let exe = Executable {
            program: "/nonexistent/rak-test-binary".into(),
            args: vec![],
        };
        assert!(run(&exe, &std::env::temp_dir(), "", Duration::from_secs(1)).is_err());
    }
}
