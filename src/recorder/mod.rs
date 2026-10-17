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
    pub fn build_viz_args(&self, device: &str, output_path: &str) -> Vec<String> {
        let mut args = vec!["-f".into(), self.input_format.into()];
        args.push("-i".into());
        args.push(device.into());
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

pub fn start_recording(output_path: &str, device: &str) -> Result<Recording, String> {
    let pc = detect_platform();
    let args = pc.build_viz_args(device, output_path);

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
    if child.id() == 0 || matches!(child.try_wait(), Ok(Some(_))) {
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

/// An audio input device ffmpeg can record from.
#[derive(Debug, Clone, PartialEq)]
pub struct InputDevice {
    /// Value passed to ffmpeg's `-i`.
    pub id: String,
    pub name: String,
}

/// Lists the audio input devices currently available. Empty if listing fails.
pub fn list_input_devices() -> Vec<InputDevice> {
    match std::env::consts::OS {
        "macos" => Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-f",
                "avfoundation",
                "-list_devices",
                "true",
                "-i",
                "",
            ])
            .output()
            .map(|o| parse_avfoundation_devices(&String::from_utf8_lossy(&o.stderr)))
            .unwrap_or_default(),
        "windows" => Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-list_devices",
                "true",
                "-f",
                "dshow",
                "-i",
                "dummy",
            ])
            .output()
            .map(|o| parse_dshow_devices(&String::from_utf8_lossy(&o.stderr)))
            .unwrap_or_default(),
        _ => Command::new("pactl")
            .args(["list", "short", "sources"])
            .output()
            .map(|o| parse_pactl_sources(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default(),
    }
}

fn parse_avfoundation_devices(output: &str) -> Vec<InputDevice> {
    let re = regex::Regex::new(r"\]\s+\[(\d+)\]\s+(.+)$").unwrap();
    let mut in_audio = false;
    let mut devices = Vec::new();
    for line in output.lines() {
        if line.contains("AVFoundation audio devices") {
            in_audio = true;
        } else if line.contains("AVFoundation video devices") {
            in_audio = false;
        } else if in_audio && let Some(caps) = re.captures(line.trim_end()) {
            devices.push(InputDevice {
                id: format!(":{}", &caps[1]),
                name: caps[2].to_string(),
            });
        }
    }
    devices
}

fn parse_dshow_devices(output: &str) -> Vec<InputDevice> {
    let re = regex::Regex::new(r#""([^"]+)"\s+\(audio\)"#).unwrap();
    output
        .lines()
        .filter_map(|l| re.captures(l))
        .map(|caps| InputDevice {
            id: format!("audio={}", &caps[1]),
            name: caps[1].to_string(),
        })
        .collect()
}

fn parse_pactl_sources(output: &str) -> Vec<InputDevice> {
    let mut devices = vec![InputDevice {
        id: "default".into(),
        name: "default".into(),
    }];
    for name in output.lines().filter_map(|l| l.split('\t').nth(1)) {
        if !name.ends_with(".monitor") {
            devices.push(InputDevice {
                id: name.into(),
                name: name.into(),
            });
        }
    }
    devices
}

/// The device after `current_id` in `devices`, wrapping around; the first one if
/// `current_id` is no longer listed.
pub fn next_device<'a>(devices: &'a [InputDevice], current_id: &str) -> Option<&'a InputDevice> {
    match devices.iter().position(|d| d.id == current_id) {
        Some(i) => devices.get((i + 1) % devices.len()),
        None => devices.first(),
    }
}

/// Each device switch starts a new ffmpeg process writing its own segment file.
pub fn segment_path(output_path: &str, index: usize) -> String {
    format!("{output_path}.seg{index}.mp3")
}

pub fn remove_segments(output_path: &str, count: usize) {
    for i in 0..count {
        let _ = fs::remove_file(segment_path(output_path, i));
    }
}

/// Joins the recorded segments into `output_path` and deletes them.
pub fn finalize_segments(output_path: &str, count: usize) -> Result<(), String> {
    let segments: Vec<String> = (0..count)
        .map(|i| segment_path(output_path, i))
        .filter(|p| fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
        .collect();
    let result = match segments.as_slice() {
        [] => Err("no audio was recorded".to_string()),
        [only] => {
            fs::rename(only, output_path).map_err(|e| format!("failed to save recording: {e}"))
        }
        _ => concat_segments(&segments, output_path),
    };
    remove_segments(output_path, count);
    result
}

fn concat_segments(segments: &[String], output_path: &str) -> Result<(), String> {
    let list_path = format!("{output_path}.concat.txt");
    let list: String = segments
        .iter()
        .map(|p| format!("file '{}'\n", p.replace('\'', "'\\''")))
        .collect();
    fs::write(&list_path, list).map_err(|e| format!("failed to write concat list: {e}"))?;
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &list_path,
            "-c",
            "copy",
            output_path,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = fs::remove_file(&list_path);
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err("failed to join recording segments".to_string()),
    }
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
        let args = pc.build_viz_args("default", "/tmp/out.mp3");
        assert_eq!(args[0], "-f");
        assert_eq!(args[1], "pulse");
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"[0:a]asplit=2[a][b]".to_string()));
        assert!(args.contains(&"pipe:1".to_string()));
    }

    #[test]
    fn parses_avfoundation_audio_devices_only() {
        let out = "[AVFoundation indev @ 0x1] AVFoundation video devices:\n\
[AVFoundation indev @ 0x1] [0] FaceTime HD Camera\n\
[AVFoundation indev @ 0x1] AVFoundation audio devices:\n\
[AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n\
[AVFoundation indev @ 0x1] [1] AirPods\n";
        let d = parse_avfoundation_devices(out);
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].id, ":1");
        assert_eq!(d[1].name, "AirPods");
    }

    #[test]
    fn parses_dshow_audio_devices() {
        let out = "[dshow @ 0x1] \"Cam\" (video)\n[dshow @ 0x1] \"Mic (USB)\" (audio)\n[dshow @ 0x1]   Alternative name \"@device\"\n";
        let d = parse_dshow_devices(out);
        assert_eq!(
            d,
            vec![InputDevice {
                id: "audio=Mic (USB)".into(),
                name: "Mic (USB)".into()
            }]
        );
    }

    #[test]
    fn parses_pactl_sources_skipping_monitors() {
        let out = "0\talsa_output.x.monitor\tmod\n1\talsa_input.usb\tmod\n";
        let d = parse_pactl_sources(out);
        assert_eq!(
            d.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            ["default", "alsa_input.usb"]
        );
    }

    #[test]
    fn next_device_wraps_and_handles_missing() {
        let mk = |id: &str| InputDevice {
            id: id.into(),
            name: id.into(),
        };
        let devs = vec![mk("a"), mk("b")];
        assert_eq!(next_device(&devs, "a").unwrap().id, "b");
        assert_eq!(next_device(&devs, "b").unwrap().id, "a");
        assert_eq!(next_device(&devs, "gone").unwrap().id, "a");
        assert!(next_device(&[], "a").is_none());
    }

    #[test]
    fn finalize_single_segment_renames() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir
            .path()
            .join("attempt-1.mp3")
            .to_string_lossy()
            .to_string();
        fs::write(segment_path(&out, 0), "audio").unwrap();
        finalize_segments(&out, 1).unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap(), "audio");
        assert!(!Path::new(&segment_path(&out, 0)).exists());
    }

    #[test]
    fn finalize_without_audio_errors() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir
            .path()
            .join("attempt-1.mp3")
            .to_string_lossy()
            .to_string();
        assert!(finalize_segments(&out, 2).is_err());
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
