//! A recording running inside the TUI. It drives the same ffmpeg helpers as
//! the stand-alone recorder (`rak record --plain`) but is stepped by the app's
//! event loop instead of owning the terminal.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::recorder::{
    self, cancel_recording, finalize_segments, list_input_devices, next_device, pause_recording,
    remove_segments, resume_recording, segment_path, start_recording, stop_recording, tui,
};

pub struct ActiveRecording {
    child: Child,
    bands: Arc<Mutex<Vec<f64>>>,
    output_path: String,
    pub attempt: usize,
    /// The recording being appended to, when resuming.
    existing: Option<PathBuf>,
    segments: usize,
    device_id: String,
    pub device_name: String,
    can_pause: bool,
    pub paused: bool,
    started: Instant,
    acc: Duration,
    pub device_lost: bool,
    done: bool,
}

pub struct Finished {
    pub audio_path: String,
    pub attempt: usize,
    pub len: Duration,
}

impl ActiveRecording {
    /// Start recording into `problem_dir`: a new attempt, or (with `resume`)
    /// more audio for the latest one.
    pub fn start(problem_dir: &Path, resume: bool, force: bool) -> Result<ActiveRecording, String> {
        recorder::check_ffmpeg()?;
        let (attempt, existing) = if resume {
            let n = recorder::latest_attempt_number(problem_dir)
                .ok_or("no previous recording to append to; press r to start attempt-1")?;
            (n, Some(problem_dir.join(format!("attempt-{n}.mp3"))))
        } else {
            (recorder::next_attempt_number(problem_dir, force), None)
        };
        // When appending, record to a side file and merge afterwards so a
        // cancel leaves the original intact.
        let output_path = match &existing {
            Some(_) => problem_dir.join(format!("attempt-{attempt}.resume.mp3")),
            None => problem_dir.join(format!("attempt-{attempt}.mp3")),
        }
        .to_string_lossy()
        .to_string();

        let pc = recorder::detect_platform();
        let devices = list_input_devices();
        let device_id = if pc.input_device.is_empty() {
            devices
                .first()
                .map_or("audio".to_string(), |d| d.id.clone())
        } else {
            pc.input_device.to_string()
        };
        let device_name = devices
            .iter()
            .find(|d| d.id == device_id)
            .map_or(device_id.clone(), |d| d.name.clone());

        let rec = start_recording(&segment_path(&output_path, 0), &device_id)?;
        let bands = Arc::new(Mutex::new(vec![0.0; 32]));
        tui::spawn_reader(rec.pipe, Arc::clone(&bands));

        Ok(ActiveRecording {
            child: rec.child,
            bands,
            output_path,
            attempt,
            existing,
            segments: 1,
            device_id,
            device_name,
            can_pause: pc.can_pause,
            paused: false,
            started: Instant::now(),
            acc: Duration::ZERO,
            device_lost: false,
            done: false,
        })
    }

    pub fn resuming(&self) -> bool {
        self.existing.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        if self.paused {
            self.acc
        } else {
            self.acc + self.started.elapsed()
        }
    }

    /// Spectrum levels in `0..=1`, resampled to `n` bars.
    pub fn bands(&self, n: usize) -> Vec<f64> {
        let mut shared = self.bands.lock().unwrap();
        if shared.len() != n && n > 0 {
            // The reader thread sizes its output from this vector.
            *shared = vec![0.0; n];
        }
        if self.paused {
            return vec![0.0; n];
        }
        shared.clone()
    }

    /// Notice when ffmpeg stopped producing audio (the input went away).
    pub fn poll(&mut self) {
        if !self.paused && matches!(self.child.try_wait(), Ok(Some(_))) {
            self.device_lost = true;
        }
    }

    pub fn toggle_pause(&mut self) {
        if !self.can_pause {
            return;
        }
        if self.paused {
            let _ = resume_recording(&mut self.child);
            self.paused = false;
            self.started = Instant::now();
        } else {
            let _ = pause_recording(&mut self.child);
            self.acc += self.started.elapsed();
            self.paused = true;
        }
    }

    /// Move to the next input device, recording into a new segment.
    pub fn switch_device(&mut self) -> Option<String> {
        let devices = list_input_devices();
        let next = next_device(&devices, &self.device_id).cloned()?;
        if self.paused {
            let _ = resume_recording(&mut self.child);
        }
        let _ = stop_recording(&mut self.child);
        match start_recording(&segment_path(&self.output_path, self.segments), &next.id) {
            Ok(new) => {
                self.child = new.child;
                tui::spawn_reader(new.pipe, Arc::clone(&self.bands));
                self.segments += 1;
                self.device_lost = false;
                if self.paused {
                    let _ = pause_recording(&mut self.child);
                }
            }
            Err(_) => self.device_lost = true,
        }
        self.device_id = next.id;
        self.device_name = next.name.clone();
        Some(next.name)
    }

    /// Stop and save. Joins segments, and merges into the earlier recording when appending.
    pub fn stop(mut self) -> Result<Finished, String> {
        let len = self.elapsed();
        if self.paused {
            let _ = resume_recording(&mut self.child);
        }
        stop_recording(&mut self.child)?;
        finalize_segments(&self.output_path, self.segments)?;
        self.done = true;
        if !Path::new(&self.output_path).exists() {
            return Err("recording was not saved".to_string());
        }
        let audio_path = match &self.existing {
            Some(existing) => {
                recorder::append_audio(existing, Path::new(&self.output_path))?;
                existing.to_string_lossy().to_string()
            }
            None => self.output_path.clone(),
        };
        Ok(Finished {
            audio_path,
            attempt: self.attempt,
            len,
        })
    }

    /// Throw the recording away.
    pub fn cancel(mut self) {
        self.discard();
    }

    fn discard(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        if self.paused {
            let _ = resume_recording(&mut self.child);
        }
        cancel_recording(
            &mut self.child,
            &segment_path(&self.output_path, self.segments - 1),
        );
        remove_segments(&self.output_path, self.segments);
    }
}

impl Drop for ActiveRecording {
    fn drop(&mut self) {
        self.discard();
    }
}
