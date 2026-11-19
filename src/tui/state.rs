//! Application state: what is selected, what is running, and the results of
//! finished work. Rendering reads it; `input` and `actions` change it.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::data::{Disk, Problem, Workspace};
use super::recording::ActiveRecording;
use crate::judge::verdict::CaseResult;
use crate::leetcode::models::{CheckResult, QuestionSummary};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Question,
    Tests,
    Record,
    Analysis,
    Submit,
    History,
}

pub const TABS: [Tab; 6] = [
    Tab::Question,
    Tab::Tests,
    Tab::Record,
    Tab::Analysis,
    Tab::Submit,
    Tab::History,
];

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Question => "Question",
            Tab::Tests => "Tests",
            Tab::Record => "Record",
            Tab::Analysis => "Analysis",
            Tab::Submit => "Submit",
            Tab::History => "History",
        }
    }

    pub fn step(self, delta: isize) -> Tab {
        let i = TABS.iter().position(|t| *t == self).unwrap_or(0) as isize;
        TABS[(i + delta).rem_euclid(TABS.len() as isize) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Rail,
    Main,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    All,
    Due,
    Leet,
}

/// Where the app opens, from the CLI command that launched it.
pub enum Route {
    Home,
    /// `rak pull` with no problem: the LeetCode list.
    Pull {
        refresh: bool,
    },
    /// `rak pull <id|slug|today>`.
    PullSlug {
        target: String,
        force: bool,
    },
    Record {
        id: String,
        force: bool,
        resume: bool,
    },
    Test {
        id: String,
        case: Option<usize>,
        timeout_ms: Option<u64>,
        lang: Option<String>,
        debug: bool,
    },
    Analyze {
        id: String,
        provider: Option<String>,
        force: bool,
    },
    Push {
        id: String,
    },
    Next,
    Log {
        id: String,
        rating: u8,
    },
}

pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

#[derive(Clone)]
pub enum Action {
    SelectProblem(String),
    OpenDue(String),
    SelectLeet(usize),
    SetMode(Mode),
    SetTab(Tab),
    Rate(u8),
    SelectCase(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Ok,
    Err,
    Warn,
    Info,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: f64,
}

pub enum Overlay {
    None,
    Palette { input: String, idx: usize },
    Help,
}

/// A question the app is waiting on before it does something.
pub enum Confirm {
    Submit(String),
    Rescaffold { slug: String, folder: String },
    DiscardRecording,
    QuitRecording,
    ReplaceRating { id: String, rating: u8 },
}

pub struct CaseRow {
    pub n: usize,
    pub input: String,
    pub expected: Option<String>,
    pub result: CaseResult,
    pub elapsed: Duration,
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum TestState {
    #[default]
    Idle,
    Building,
    Running,
    Done,
    Failed,
}

#[derive(Default)]
pub struct TestRun {
    pub state: TestState,
    pub started: f64,
    /// Case numbers this run covers, in order.
    pub numbers: Vec<usize>,
    pub rows: Vec<CaseRow>,
    pub build: Option<Duration>,
    pub error: Option<String>,
    pub debug: bool,
    /// Re-running one case: keep the other rows.
    pub partial: bool,
}

impl TestRun {
    pub fn failed_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.result.verdict.is_failure())
            .count()
    }
}

#[derive(Clone)]
pub enum PipeStage {
    Transcribing,
    Analyzing,
    Done,
    Failed(String),
}

/// What happens after a recording stops: transcribe, then analyze.
pub struct Pipe {
    pub attempt: usize,
    pub len: Duration,
    pub stage: PipeStage,
}

#[derive(Default)]
pub enum SubmitRun {
    #[default]
    Idle,
    Working(String),
    Done(Box<CheckResult>),
    Failed(String),
}

pub struct Session {
    pub disk: Option<Disk>,
    pub test: TestRun,
    pub submit: SubmitRun,
    pub pipe: Option<Pipe>,
    /// Start time of a standalone analysis run.
    pub analyzing: Option<f64>,
    pub analysis_error: Option<String>,
    pub scroll_question: usize,
    pub scroll_analysis: usize,
    pub case_sel: usize,
    pub rate_hi: u8,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            disk: None,
            test: TestRun::default(),
            submit: SubmitRun::Idle,
            pipe: None,
            analyzing: None,
            analysis_error: None,
            scroll_question: 0,
            scroll_analysis: 0,
            case_sel: 0,
            rate_hi: 4,
        }
    }
}

pub enum LeetList {
    Unloaded,
    Loading,
    Ready(Vec<QuestionSummary>),
    Failed(String),
}

pub enum Detail {
    Loading,
    Ready(String),
    Failed(String),
}

pub struct Leet {
    pub query: String,
    pub idx: usize,
    pub list: LeetList,
    /// Indexes into the ready list that match `query`.
    pub filtered: Vec<usize>,
    pub details: HashMap<String, Detail>,
    /// A description fetch waiting for the selection to settle: (slug, since).
    pub want: Option<(String, f64)>,
}

impl Leet {
    pub fn new() -> Leet {
        Leet {
            query: String::new(),
            idx: 0,
            list: LeetList::Unloaded,
            filtered: Vec::new(),
            details: HashMap::new(),
            want: None,
        }
    }

    pub fn refilter(&mut self) {
        let LeetList::Ready(items) = &self.list else {
            self.filtered.clear();
            return;
        };
        let q = self.query.trim().to_lowercase();
        self.filtered = items
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.title.to_lowercase().contains(&q)
                    || p.frontend_id.starts_with(&q)
                    || p.title_slug.contains(&q)
                    || p.topic_tags
                        .iter()
                        .any(|t| t.name.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect();
        self.idx = self.idx.min(self.filtered.len().saturating_sub(1));
    }

    pub fn current(&self) -> Option<&QuestionSummary> {
        let LeetList::Ready(items) = &self.list else {
            return None;
        };
        self.filtered.get(self.idx).and_then(|&i| items.get(i))
    }
}

/// Messages from background work to the UI.
pub enum Msg {
    TestBuilt(String, Duration, Vec<usize>),
    TestCase(String, CaseRow),
    TestDone(String),
    TestFailed(String, String),
    Pipe(String, PipeStage),
    SubmitStage(String, String),
    SubmitDone(String, Result<CheckResult, String>),
    AnalyzeDone(String, Result<(), String>),
    LeetList(Result<Vec<QuestionSummary>, String>),
    LeetDetail(String, Result<String, String>),
    Pulled {
        label: String,
        result: Result<crate::commands::pull::Scaffolded, String>,
    },
}

pub struct Rec {
    pub id: String,
    pub active: ActiveRecording,
}

pub struct App {
    pub ws: Workspace,
    pub config_dir: PathBuf,
    pub leetcode_dir: PathBuf,
    pub analyze_provider: String,
    pub transcribe_provider: String,
    pub mode: Mode,
    pub sel: Option<String>,
    pub focus: Focus,
    pub tab: Tab,
    pub sessions: HashMap<String, Session>,
    pub rec: Option<Rec>,
    pub leet: Leet,
    pub overlay: Overlay,
    pub confirm: Option<Confirm>,
    pub toast: Option<Toast>,
    /// Slugs being pulled right now, for the footer.
    pub pulling: Vec<String>,
    pub tx: Sender<Msg>,
    pub quit: bool,
    pub pending_edit: Option<PathBuf>,
    pub started: Instant,
    /// Largest scroll offset of the text on screen; set while drawing.
    pub max_scroll: Cell<usize>,
    pub hits: Vec<Hit>,
}

impl App {
    pub fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let until = self.now() + 4.5;
        self.toast = Some(Toast {
            text: text.into(),
            kind,
            until,
        });
    }

    pub fn problem(&self) -> Option<&Problem> {
        self.sel.as_deref().and_then(|id| self.ws.find(id))
    }

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn session_mut(&mut self, id: &str) -> &mut Session {
        self.sessions.entry(id.to_string()).or_default()
    }

    /// Problems shown in the sidebar for the current mode.
    pub fn visible(&self) -> Vec<&Problem> {
        match self.mode {
            Mode::Due => {
                let mut v: Vec<&Problem> = self.ws.problems.iter().filter(|p| p.is_due()).collect();
                v.sort_by_key(|p| std::cmp::Reverse(p.due));
                v
            }
            _ => self.ws.problems.iter().collect(),
        }
    }

    pub fn recording_id(&self) -> Option<&str> {
        self.rec.as_ref().map(|r| r.id.as_str())
    }

    /// The rating widget is on screen for the selected problem.
    pub fn rating_visible(&self) -> bool {
        let Some(p) = self.problem() else {
            return false;
        };
        if self.mode == Mode::Leet || self.overlay_open() {
            return false;
        }
        let Some(s) = self.session(&p.id) else {
            return self.tab == Tab::History;
        };
        match self.tab {
            Tab::History => true,
            Tab::Record => {
                matches!(
                    s.pipe,
                    Some(Pipe {
                        stage: PipeStage::Done | PipeStage::Failed(_),
                        ..
                    })
                ) && self.recording_id() != Some(p.id.as_str())
            }
            Tab::Submit => matches!(&s.submit, SubmitRun::Done(r) if r.status_msg == "Accepted"),
            _ => false,
        }
    }

    pub fn overlay_open(&self) -> bool {
        !matches!(self.overlay, Overlay::None) || self.confirm.is_some()
    }

    /// Short labels for work in progress, shown in the footer.
    pub fn jobs(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (id, s) in &self.sessions {
            match s.test.state {
                TestState::Building => out.push(format!("building {id}")),
                TestState::Running => out.push(format!(
                    "testing {id} {}/{}",
                    s.test.rows.len(),
                    s.test.numbers.len()
                )),
                _ => {}
            }
            match s.pipe.as_ref().map(|p| &p.stage) {
                Some(PipeStage::Transcribing) => out.push(format!("transcribing {id}")),
                Some(PipeStage::Analyzing) => out.push(format!("analyzing {id}")),
                _ => {}
            }
            if s.analyzing.is_some() {
                out.push(format!("analyzing {id}"));
            }
            if let SubmitRun::Working(stage) = &s.submit {
                out.push(format!("{} · {id}", stage.to_lowercase()));
            }
        }
        for slug in &self.pulling {
            out.push(format!("pulling {slug}"));
        }
        if matches!(self.leet.list, LeetList::Loading) {
            out.push("loading problems".to_string());
        }
        out.sort();
        out
    }

    pub fn tab_busy(&self, tab: Tab) -> Option<bool> {
        // Some(true) = recording (red dot), Some(false) = working (accent dot).
        let id = self.sel.as_deref()?;
        let s = self.session(id)?;
        match tab {
            Tab::Record => {
                if self.recording_id() == Some(id) {
                    Some(true)
                } else if matches!(
                    s.pipe.as_ref().map(|p| &p.stage),
                    Some(PipeStage::Transcribing | PipeStage::Analyzing)
                ) {
                    Some(false)
                } else {
                    None
                }
            }
            Tab::Tests => {
                matches!(s.test.state, TestState::Building | TestState::Running).then_some(false)
            }
            Tab::Submit => matches!(s.submit, SubmitRun::Working(_)).then_some(false),
            Tab::Analysis => s.analyzing.is_some().then_some(false),
            _ => None,
        }
    }
}
