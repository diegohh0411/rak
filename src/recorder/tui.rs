use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use super::fft;
use super::{
    cancel_recording, detect_platform, pause_recording, resume_recording, start_recording,
    stop_recording,
};

const PCM_CHUNK_SIZE: usize = 2048;
const TICK_MS: u64 = 100;

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Recording,
    Paused,
    Stopping,
    Done,
    Cancelled,
    Error(String),
}

const BLOCK_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn num_bands_for_width(w: usize) -> usize {
    if w == 0 {
        return 16;
    }
    let n = w.div_ceil(2);
    if n < 4 {
        4
    } else {
        n
    }
}

pub fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

pub fn render_eq(bands: &[f64]) -> String {
    let mut result = String::with_capacity(bands.len() * 2);
    for (i, &level) in bands.iter().enumerate() {
        let idx = (level * (BLOCK_CHARS.len() - 1) as f64) as usize;
        let idx = idx.clamp(0, BLOCK_CHARS.len() - 1);
        result.push(BLOCK_CHARS[idx]);
        if i < bands.len() - 1 {
            result.push(' ');
        }
    }
    result
}

fn read_pcm_chunk(pipe: &mut Box<dyn Read + Send>, num_bands: usize) -> Option<Vec<f64>> {
    let mut buf = vec![0u8; PCM_CHUNK_SIZE * 2];
    match pipe.read_exact(&mut buf) {
        Ok(()) => {}
        Err(_) => return None,
    }

    let mut samples = vec![0.0f64; PCM_CHUNK_SIZE];
    for i in 0..PCM_CHUNK_SIZE {
        let byte_lo = buf[i * 2] as i16;
        let byte_hi = buf[i * 2 + 1] as i16;
        let sample = (byte_lo) | (byte_hi << 8);
        samples[i] = sample as f64 / 32768.0;
    }

    Some(fft::analyze_bands(&samples, num_bands))
}

pub fn run_recorder_tui(output_path: &str, filename: &str) -> Result<Option<String>, String> {
    let mut recording = start_recording(output_path)?;

    let can_pause = detect_platform().can_pause;
    let num_bands = num_bands_for_width(80);
    let shared_bands: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(vec![0.0; num_bands]));
    let bands_clone = Arc::clone(&shared_bands);
    let pipe: Arc<Mutex<Box<dyn Read + Send>>> = Arc::new(Mutex::new(recording.pipe));

    let pipe_reader = std::thread::spawn(move || loop {
        let nb = {
            let bands = bands_clone.lock().unwrap();
            bands.len()
        };
        let mut pipe_guard = match pipe.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let result = read_pcm_chunk(&mut pipe_guard, nb);
        drop(pipe_guard);
        match result {
            Some(bands) => {
                let mut shared = bands_clone.lock().unwrap();
                *shared = bands;
            }
            None => return,
        }
    });

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal init: {e}"))?;

    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    crossterm::execute!(std::io::stdout(), EnterAlternateScreen)
        .map_err(|e| format!("alternate screen: {e}"))?;
    terminal.clear().map_err(|e| format!("clear: {e}"))?;

    let mut status = Status::Recording;
    let mut start = Instant::now();
    let mut elapsed = Duration::ZERO;
    let mut width: usize = 80;
    let result = 'outer: loop {
        if event::poll(Duration::from_millis(TICK_MS)).map_err(|e| format!("poll: {e}"))?
            && let Event::Key(key) = event::read().map_err(|e| format!("read event: {e}"))?
        {
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) | (KeyCode::Enter, _) => {
                    status = Status::Stopping;
                    let _ = terminal.draw(|frame| {
                        let area = frame.area();
                        frame.render_widget(view(&status, elapsed, filename, &[], width), area);
                    });
                    drop(pipe_reader);
                    let stop_result = stop_recording(&mut recording.child);
                    if let Err(e) = stop_result {
                        status = Status::Error(e);
                    } else {
                        status = Status::Done;
                    }
                    let _ = terminal.draw(|frame| {
                        let area = frame.area();
                        frame.render_widget(view(&status, elapsed, filename, &[], width), area);
                    });
                    break 'outer match status {
                        Status::Done => Ok(Some(output_path.to_string())),
                        Status::Error(e) => Err(e),
                        _ => Ok(None),
                    };
                }
                (KeyCode::Char(' '), _) => {
                    if can_pause && status == Status::Recording {
                        let _ = pause_recording(&mut recording.child);
                        status = Status::Paused;
                    } else if can_pause && status == Status::Paused {
                        let _ = resume_recording(&mut recording.child);
                        status = Status::Recording;
                        start = Instant::now() - elapsed;
                    }
                }
                (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                    status = Status::Cancelled;
                    cancel_recording(&mut recording.child, output_path);
                    let _ = terminal.draw(|frame| {
                        let area = frame.area();
                        frame.render_widget(view(&status, elapsed, filename, &[], width), area);
                    });
                    break 'outer Ok(None);
                }
                _ => {}
            }
        }

        if let Ok(new_size) = terminal.size() {
            width = new_size.width as usize;
        }

        if status == Status::Recording {
            elapsed = start.elapsed();
        }

        let bands = shared_bands.lock().unwrap().clone();
        let _ = terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(view(&status, elapsed, filename, &bands, width), area);
        });
    };

    disable_raw_mode().ok();
    crossterm::execute!(std::io::stdout(), LeaveAlternateScreen).ok();

    result
}

fn view(
    status: &Status,
    elapsed: Duration,
    filename: &str,
    bands: &[f64],
    width: usize,
) -> Paragraph<'static> {
    match status {
        Status::Stopping => {
            let line = Line::from(Span::styled(
                format!("Saving {filename}..."),
                Style::default().fg(Color::Yellow),
            ));
            return Paragraph::new(vec![line]);
        }
        Status::Done => {
            let line = Line::from(Span::styled(
                format!("✓ Saved {filename} ({})", format_duration(elapsed)),
                Style::default().fg(Color::Green),
            ));
            return Paragraph::new(vec![line]);
        }
        Status::Cancelled => {
            let line = Line::from(Span::styled(
                "Cancelled — partial file discarded.",
                Style::default().fg(Color::Yellow),
            ));
            return Paragraph::new(vec![line]);
        }
        Status::Error(e) => {
            let line = Line::from(Span::styled(
                format!("Error: {e}"),
                Style::default().fg(Color::Red),
            ));
            return Paragraph::new(vec![line]);
        }
        _ => {}
    }

    let (indicator_char, indicator_color, controls) = match status {
        Status::Recording => ("⏺", Color::Red, "[space] pause  [q] stop  [ctrl+c] cancel"),
        Status::Paused => (
            "⏸",
            Color::Yellow,
            "[space] resume  [q] stop  [ctrl+c] cancel",
        ),
        _ => unreachable!(),
    };

    let num_bands = num_bands_for_width(width);
    let _ = num_bands;
    let eq = render_eq(bands);

    let header = Line::from(vec![
        Span::styled(
            indicator_char.to_string(),
            Style::default()
                .fg(indicator_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            " {}  Recording {}",
            format_duration(elapsed),
            filename
        )),
    ]);

    let eq_line = Line::from(Span::raw(eq));

    let controls_line = Line::from(Span::styled(
        controls,
        Style::default().add_modifier(Modifier::DIM),
    ));

    Paragraph::new(vec![header, eq_line, controls_line])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_bands_for_width_test() {
        assert_eq!(num_bands_for_width(0), 16);
        assert_eq!(num_bands_for_width(80), 40);
        assert_eq!(num_bands_for_width(7), 4);
    }

    #[test]
    fn render_eq_test() {
        let bands = vec![0.0, 0.5, 1.0];
        let s = render_eq(&bands);
        assert!(s.contains('█'));
        assert!(s.contains('▁'));
    }

    #[test]
    fn format_duration_test() {
        assert_eq!(format_duration(Duration::from_secs(0)), "00:00");
        assert_eq!(format_duration(Duration::from_secs(23)), "00:23");
        assert_eq!(format_duration(Duration::from_secs(125)), "02:05");
    }
}
