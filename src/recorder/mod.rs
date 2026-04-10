pub mod fft;
pub mod tui;

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub struct PlatformConfig {
    pub input_format: &'static str,
    pub input_device: &'static str,
    pub can_pause: bool,
}

pub fn detect_platform() -> &'static PlatformConfig {
    detect_platform_for(std::env::consts::OS)
}

pub fn detect_platform_for(os: &str) -> &'static PlatformConfig {
    static DARWIN: PlatformConfig = PlatformConfig {
        input_format: "avfoundation",
        input_device: ":0",
        can_pause: true,
    };
    static LINUX: PlatformConfig = PlatformConfig {
        input_format: "pulse",
        input_device: "default",
        can_pause: true,
    };
    static WINDOWS: PlatformConfig = PlatformConfig {
        input_format: "dshow",
        input_device: "",
        can_pause: false,
    };

    match os {
        "macos" => &DARWIN,
        "windows" => &WINDOWS,
        _ => &LINUX,
    }
}

impl PlatformConfig {
    pub fn build_viz_args(&self, output_path: &str) -> Vec<String> {
        let mut args = vec!["-f".into(), self.input_format.into()];
        if !self.input_device.is_empty() {
            args.push("-i".into());
            args.push(self.input_device.into());
        } else {
            args.push("-i".into());
            args.push("audio".into());
        }
        args.extend([
            "-filter_complex".into(),
            "[0:a]asplit=2[a][b]".into(),
            "-map".into(),
            "[a]".into(),
            "-c:a".into(),
            "libmp3lame".into(),
            "-q:a".into(),
            "2".into(),
            "-y".into(),
            output_path.into(),
            "-map".into(),
            "[b]".into(),
            "-f".into(),
            "s16le".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "44100".into(),
            "pipe:1".into(),
        ]);
        args
    }
}

pub fn check_ffmpeg() -> Result<(), String> {
    let result = if cfg!(windows) {
        Command::new("where").arg("ffmpeg").output()
    } else {
        Command::new("which").arg("ffmpeg").output()
    };
    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err("ffmpeg is not installed or not on PATH.\n\nInstall it with:\n  macOS:   brew install ffmpeg\n  Linux:   sudo apt install ffmpeg\n  Windows: winget install ffmpeg\n  See: https://ffmpeg.org/download.html".to_string()),
    }
}

pub struct Recording {
    pub child: Child,
    pub pipe: Box<dyn Read + Send>,
}

pub fn start_recording(output_path: &str) -> Result<Recording, String> {
    let pc = detect_platform();
    let args = pc.build_viz_args(output_path);

    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(std::process::Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn ffmpeg: {e}"))?;
    let pipe = child
        .stdout
        .take()
        .ok_or("failed to get ffmpeg stdout pipe")?;

    Ok(Recording {
        child,
        pipe: Box::new(pipe),
    })
}

pub fn stop_recording(child: &mut Child) -> Result<(), String> {
    if child.id() == 0 {
        return Ok(());
    }
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGINT);
        }
    }
    #[cfg(windows)]
    {
        child
            .kill()
            .map_err(|e| format!("failed to kill ffmpeg: {e}"))?;
    }
    match child.wait() {
        Ok(_) => Ok(()),
        Err(_) => Ok(()),
    }
}

pub fn pause_recording(child: &mut Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGSTOP);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = child;
        Err("pause is not supported on Windows".to_string())
    }
}

pub fn resume_recording(child: &mut Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGCONT);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = child;
        Err("resume is not supported on Windows".to_string())
    }
}

pub fn cancel_recording(child: &mut Child, output_path: &str) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(output_path);
}

pub fn next_attempt_number(dir: &Path, force: bool) -> usize {
    if force {
        return 1;
    }
    let re = regex::Regex::new(r"^attempt-(\d+)\.mp3$").unwrap();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 1,
    };
    let mut max_n = 0;
    for entry in entries.flatten() {
        if let Some(caps) = re.captures(&entry.file_name().to_string_lossy())
            && let Ok(n) = caps[1].parse::<usize>()
            && n > max_n
        {
            max_n = n;
        }
    }
    max_n + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform_known_os() {
        let darwin = detect_platform_for("macos");
        assert_eq!(darwin.input_format, "avfoundation");
        assert_eq!(darwin.input_device, ":0");
        assert!(darwin.can_pause);

        let linux = detect_platform_for("linux");
        assert_eq!(linux.input_format, "pulse");
        assert!(linux.can_pause);

        let win = detect_platform_for("windows");
        assert_eq!(win.input_format, "dshow");
        assert!(!win.can_pause);
    }

    #[test]
    fn build_viz_args_structure() {
        let pc = PlatformConfig {
            input_format: "pulse",
            input_device: "default",
            can_pause: true,
        };
        let args = pc.build_viz_args("/tmp/out.mp3");
        assert_eq!(args[0], "-f");
        assert_eq!(args[1], "pulse");
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"[0:a]asplit=2[a][b]".to_string()));
        assert!(args.contains(&"pipe:1".to_string()));
    }

    #[test]
    fn next_attempt_number_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(next_attempt_number(dir.path(), false), 1);
    }

    #[test]
    fn next_attempt_number_with_existing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-1.mp3"), "").unwrap();
        fs::write(dir.path().join("attempt-3.mp3"), "").unwrap();
        assert_eq!(next_attempt_number(dir.path(), false), 4);
    }

    #[test]
    fn next_attempt_number_force_returns_one() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-5.mp3"), "").unwrap();
        assert_eq!(next_attempt_number(dir.path(), true), 1);
    }
}
