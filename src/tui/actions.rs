//! Things the user can do: every key, palette command and CLI route ends up
//! calling one of these.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Instant;

use super::data::{self, Disk, Workspace};
use super::jobs::{self, TestArgs};
use super::recording::ActiveRecording;
use super::state::*;
use crate::commands::pull::Scaffolded;
use crate::config;

impl App {
    pub fn new(
        ws: Workspace,
        config_dir: PathBuf,
        cfg: &config::RakConfig,
        tx: Sender<Msg>,
    ) -> App {
        App {
            ws,
            leetcode_dir: config_dir.join(&cfg.leetcode_dir),
            config_dir,
            analyze_provider: cfg.analyze.default_provider.clone(),
            transcribe_provider: cfg.transcribe.default_provider.clone(),
            mode: Mode::All,
            sel: None,
            focus: Focus::Rail,
            tab: Tab::Question,
            sessions: Default::default(),
            rec: None,
            leet: Leet::new(),
            overlay: Overlay::None,
            confirm: None,
            toast: None,
            pulling: Vec::new(),
            tx,
            quit: false,
            pending_edit: None,
            started: Instant::now(),
            max_scroll: Default::default(),
            hits: Vec::new(),
        }
    }

    // ---- selection ----------------------------------------------------

    pub fn select(&mut self, id: &str) {
        self.sel = Some(id.to_string());
    }

    pub fn folder_of(&self, id: &str) -> Option<PathBuf> {
        self.ws.find(id).and_then(|p| p.folder.clone())
    }

    /// Read the selected problem's files once; reread after anything changes them.
    pub fn ensure_disk(&mut self) {
        let Some(id) = self.sel.clone() else { return };
        let Some(folder) = self.folder_of(&id) else {
            return;
        };
        let s = self.session_mut(&id);
        if s.disk.is_none() {
            s.disk = Some(Disk::load(&folder));
        }
    }

    pub fn invalidate_disk(&mut self, id: &str) {
        self.session_mut(id).disk = None;
    }

    pub fn reload_workspace(&mut self) {
        if let Err(e) = self.ws.reload() {
            self.toast(ToastKind::Err, e);
        }
    }

    /// Resolve what the user typed ("54", a custom slug) to a problem in the list.
    pub fn resolve(&self, id: &str) -> Result<String, String> {
        if self.ws.find(id).is_some() {
            return Ok(id.to_string());
        }
        let folder = config::resolve_problem_folder(
            &self.config_dir,
            &config::load(&self.config_dir)?,
            &config::ProblemId::parse(id),
        )?;
        let name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let key = crate::commands::analyze::folder_to_history_key(name);
        self.ws
            .find(&key)
            .map(|p| p.id.clone())
            .ok_or_else(|| format!("{id} is not in the problem list"))
    }

    // ---- routes -------------------------------------------------------

    pub fn apply_route(&mut self, route: Route) {
        match route {
            Route::Home => {}
            Route::Pull { refresh } => self.open_leet(refresh),
            Route::PullSlug { target, force } => {
                let label = target.clone();
                self.pull_target(target, force, label);
            }
            Route::Record { id, force, resume } => {
                if let Some(id) = self.open_problem(&id, Tab::Record) {
                    self.start_record(&id, resume, force);
                }
            }
            Route::Test {
                id,
                case,
                timeout_ms,
                lang,
                debug,
            } => {
                if let Some(id) = self.open_problem(&id, Tab::Tests) {
                    self.start_test(
                        &id,
                        TestArgs {
                            case,
                            timeout_ms,
                            lang,
                            debug,
                        },
                    );
                }
            }
            Route::Analyze {
                id,
                provider,
                force,
            } => {
                if let Some(id) = self.open_problem(&id, Tab::Analysis) {
                    self.ensure_disk();
                    let exists = self
                        .session(&id)
                        .and_then(|s| s.disk.as_ref())
                        .is_some_and(|d| d.analysis.is_some());
                    if exists && !force {
                        self.toast(ToastKind::Info, "analysis.md exists · a runs it again");
                    } else {
                        self.start_analyze(&id, provider, force);
                    }
                }
            }
            Route::Push { id } => {
                if let Some(id) = self.open_problem(&id, Tab::Submit) {
                    self.start_submit(&id);
                }
            }
            Route::Next => self.next_due(),
            Route::Log { id, rating } => {
                if let Some(id) = self.open_problem(&id, Tab::History) {
                    self.save_rating(&id, rating, false);
                }
            }
        }
    }

    /// Select a problem and show `tab`, or explain why that can't be done.
    fn open_problem(&mut self, id: &str, tab: Tab) -> Option<String> {
        match self.resolve(id) {
            Ok(id) => {
                self.mode = Mode::All;
                self.select(&id);
                self.tab = tab;
                self.focus = Focus::Main;
                Some(id)
            }
            Err(e) => {
                self.toast(ToastKind::Err, e);
                None
            }
        }
    }

    pub fn next_due(&mut self) {
        let due: Vec<String> = {
            let mut v: Vec<&data::Problem> =
                self.ws.problems.iter().filter(|p| p.is_due()).collect();
            v.sort_by_key(|p| std::cmp::Reverse(p.due));
            v.iter().map(|p| p.id.clone()).collect()
        };
        if due.is_empty() {
            self.toast(ToastKind::Info, "Nothing due today");
            return;
        }
        let at = self
            .sel
            .as_ref()
            .and_then(|s| due.iter().position(|d| d == s));
        let next = match at {
            Some(i) => due[(i + 1) % due.len()].clone(),
            None => due[0].clone(),
        };
        self.mode = Mode::Due;
        self.select(&next);
        self.tab = Tab::History;
        self.focus = Focus::Main;
    }

    // ---- tests --------------------------------------------------------

    pub fn start_test(&mut self, id: &str, args: TestArgs) {
        if self.folder_of(id).is_none() {
            return self.toast(ToastKind::Err, format!("{id} has no folder yet"));
        }
        let now = self.now();
        let s = self.session_mut(id);
        s.test = TestRun {
            state: TestState::Building,
            started: now,
            debug: args.debug,
            ..Default::default()
        };
        s.case_sel = 0;
        self.tab = Tab::Tests;
        self.focus = Focus::Main;
        jobs::test(self.tx.clone(), id.to_string(), args);
    }

    // ---- recording ----------------------------------------------------

    pub fn start_record(&mut self, id: &str, resume: bool, force: bool) {
        if let Some(r) = &self.rec {
            let msg = if r.id == id {
                "Already recording this problem".to_string()
            } else {
                format!("Already recording {}", r.id)
            };
            return self.toast(ToastKind::Warn, msg);
        }
        let Some(folder) = self.folder_of(id) else {
            return self.toast(ToastKind::Err, format!("{id} has no folder yet"));
        };
        match ActiveRecording::start(&folder, resume, force) {
            Ok(active) => {
                self.session_mut(id).pipe = None;
                self.rec = Some(Rec {
                    id: id.to_string(),
                    active,
                });
                self.tab = Tab::Record;
                self.focus = Focus::Main;
            }
            Err(e) => self.toast(ToastKind::Err, e),
        }
    }

    pub fn stop_record(&mut self) {
        let Some(rec) = self.rec.take() else { return };
        let id = rec.id.clone();
        match rec.active.stop() {
            Ok(done) => {
                let s = self.session_mut(&id);
                s.pipe = Some(Pipe {
                    attempt: done.attempt,
                    len: done.len,
                    stage: PipeStage::Transcribing,
                });
                self.invalidate_disk(&id);
                jobs::pipeline(self.tx.clone(), id, done.audio_path, done.attempt);
            }
            Err(e) => self.toast(ToastKind::Err, e),
        }
    }

    pub fn discard_record(&mut self) {
        if let Some(rec) = self.rec.take() {
            rec.active.cancel();
            self.toast(ToastKind::Warn, "Discarded the recording");
        }
    }

    // ---- analysis and submission ------------------------------------

    pub fn start_analyze(&mut self, id: &str, provider: Option<String>, force: bool) {
        if self.folder_of(id).is_none() {
            return self.toast(ToastKind::Err, format!("{id} has no folder yet"));
        }
        let now = self.now();
        let s = self.session_mut(id);
        s.analyzing = Some(now);
        s.analysis_error = None;
        self.tab = Tab::Analysis;
        self.focus = Focus::Main;
        jobs::analyze(self.tx.clone(), id.to_string(), provider, force);
    }

    pub fn start_submit(&mut self, id: &str) {
        if self.folder_of(id).is_none() {
            return self.toast(ToastKind::Err, format!("{id} has no folder yet"));
        }
        self.session_mut(id).submit = SubmitRun::Working("Reading solution".to_string());
        self.tab = Tab::Submit;
        self.focus = Focus::Main;
        jobs::submit(self.tx.clone(), id.to_string());
    }

    // ---- pull ---------------------------------------------------------

    pub fn open_leet(&mut self, refresh: bool) {
        self.mode = Mode::Leet;
        self.focus = Focus::Rail;
        if matches!(self.leet.list, LeetList::Unloaded | LeetList::Failed(_)) || refresh {
            self.leet.list = LeetList::Loading;
            jobs::leet_list(self.tx.clone(), refresh);
        }
    }

    pub fn pull_target(&mut self, target: String, force: bool, label: String) {
        self.pulling.push(label.clone());
        jobs::pull(self.tx.clone(), target, force, false, label);
    }

    /// Enter on a LeetCode result: open the folder if it exists, else pull it.
    pub fn leet_enter(&mut self) {
        let Some(p) = self.leet.current().cloned() else {
            return;
        };
        if let Some(local) = self.ws.find(&p.frontend_id).filter(|l| l.folder.is_some()) {
            let id = local.id.clone();
            self.mode = Mode::All;
            self.select(&id);
            self.tab = Tab::Question;
            self.focus = Focus::Main;
        } else {
            self.pull_target(p.title_slug.clone(), false, p.frontend_id.clone());
        }
    }

    pub fn leet_force(&mut self) {
        let Some(p) = self.leet.current().cloned() else {
            return;
        };
        match self.ws.find(&p.frontend_id).filter(|l| l.folder.is_some()) {
            Some(_) => {
                let folder = p.folder_name();
                self.confirm = Some(Confirm::Rescaffold {
                    slug: p.title_slug,
                    folder,
                })
            }
            None => self.toast(ToastKind::Info, "Nothing to overwrite · ↵ pulls it"),
        }
    }

    // ---- rating -------------------------------------------------------

    pub fn save_rating(&mut self, id: &str, rating: u8, replace: bool) {
        let rated = self.ws.find(id).and_then(|p| p.rated_today(self.ws.today));
        if rated.is_some() && !replace {
            self.confirm = Some(Confirm::ReplaceRating {
                id: id.to_string(),
                rating,
            });
            return;
        }
        match crate::commands::log::record(id, rating, rated.is_some(), None) {
            Ok(done) => {
                self.reload_workspace();
                let days = data::interval(done.new_box);
                let when = (done.date + chrono::Duration::days(days))
                    .format("%a %b %-d")
                    .to_string();
                self.toast(
                    ToastKind::Ok,
                    format!("Rated {rating} → box {} · next review {when}", done.new_box),
                );
            }
            Err(e) => self.toast(ToastKind::Err, e),
        }
    }

    // ---- background messages ----------------------------------------

    pub fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::TestBuilt(id, build, numbers) => {
                let s = self.session_mut(&id);
                s.test.state = TestState::Running;
                s.test.build = Some(build);
                if !s.test.partial {
                    s.test.numbers = numbers;
                }
            }
            Msg::TestCase(id, row) => {
                let rows = &mut self.session_mut(&id).test.rows;
                match rows.iter().position(|r| r.n == row.n) {
                    Some(i) => rows[i] = row,
                    None => rows.push(row),
                }
            }
            Msg::TestDone(id) => {
                let s = self.session_mut(&id);
                s.test.state = TestState::Done;
                let failed = s.test.failed_count();
                let total = s.test.rows.len();
                if !s.test.partial {
                    s.case_sel = s
                        .test
                        .rows
                        .iter()
                        .position(|r| r.result.verdict.is_failure())
                        .unwrap_or(0);
                }
                s.test.partial = false;
                if failed == 0 {
                    self.toast(
                        ToastKind::Ok,
                        format!("✓ {id}: {total}/{total} cases passed"),
                    );
                } else {
                    self.toast(
                        ToastKind::Err,
                        format!("✗ {id}: {}/{total} cases passed", total - failed),
                    );
                }
            }
            Msg::TestFailed(id, e) => {
                let s = self.session_mut(&id);
                s.test.state = TestState::Failed;
                s.test.error = Some(e);
                self.toast(ToastKind::Err, format!("✗ {id}: tests could not run"));
            }
            Msg::Pipe(id, stage) => {
                let toast = match &stage {
                    PipeStage::Done => Some((ToastKind::Ok, format!("✓ Analysis ready for {id}"))),
                    PipeStage::Failed(e) => Some((ToastKind::Err, e.clone())),
                    _ => None,
                };
                if let Some(p) = self.session_mut(&id).pipe.as_mut() {
                    p.stage = stage;
                }
                self.invalidate_disk(&id);
                if let Some((kind, text)) = toast {
                    self.toast(kind, text);
                }
            }
            Msg::SubmitStage(id, stage) => {
                if matches!(self.session_mut(&id).submit, SubmitRun::Working(_)) {
                    self.session_mut(&id).submit = SubmitRun::Working(stage);
                }
            }
            Msg::SubmitDone(id, result) => match result {
                Ok(r) => {
                    let ok = r.status_msg == "Accepted";
                    let text = if ok {
                        format!("✓ {id}: Accepted")
                    } else {
                        format!("✗ {id}: {}", r.status_msg)
                    };
                    self.session_mut(&id).submit = SubmitRun::Done(Box::new(r));
                    self.toast(if ok { ToastKind::Ok } else { ToastKind::Err }, text);
                }
                Err(e) => {
                    self.session_mut(&id).submit = SubmitRun::Failed(e.clone());
                    self.toast(ToastKind::Err, format!("✗ {id}: submission failed"));
                }
            },
            Msg::AnalyzeDone(id, result) => {
                let s = self.session_mut(&id);
                s.analyzing = None;
                match result {
                    Ok(()) => {
                        self.invalidate_disk(&id);
                        self.toast(ToastKind::Ok, format!("✓ Analysis ready for {id}"));
                    }
                    Err(e) => {
                        self.session_mut(&id).analysis_error = Some(e);
                        self.toast(ToastKind::Err, format!("✗ {id}: analysis failed"));
                    }
                }
            }
            Msg::LeetList(result) => match result {
                Ok(list) => {
                    self.leet.list = LeetList::Ready(list);
                    self.leet.refilter();
                }
                Err(e) => self.leet.list = LeetList::Failed(e),
            },
            Msg::LeetDetail(slug, result) => {
                let d = match result {
                    Ok(text) => Detail::Ready(text),
                    Err(e) => Detail::Failed(e),
                };
                self.leet.details.insert(slug, d);
            }
            Msg::Pulled { label, result } => {
                self.pulling.retain(|l| *l != label);
                match result {
                    Ok(Scaffolded::Created { id, folder, .. }) => {
                        self.reload_workspace();
                        self.mode = Mode::All;
                        self.select(&id);
                        self.invalidate_disk(&id);
                        self.tab = Tab::Question;
                        self.focus = Focus::Main;
                        self.toast(ToastKind::Ok, format!("✓ Pulled {id} → {folder}/"));
                    }
                    Ok(Scaffolded::Exists { id, folder }) => {
                        self.reload_workspace();
                        self.mode = Mode::All;
                        self.select(&id);
                        self.tab = Tab::Question;
                        self.focus = Focus::Main;
                        self.toast(ToastKind::Info, format!("{folder}/ already exists"));
                    }
                    Err(e) => self.toast(ToastKind::Err, e),
                }
            }
        }
    }

    /// Called every frame: timers, recording health, debounced fetches.
    pub fn tick(&mut self) {
        let now = self.now();
        if self.toast.as_ref().is_some_and(|t| now >= t.until) {
            self.toast = None;
        }
        if let Some(r) = self.rec.as_mut() {
            r.active.poll();
        }
        self.ensure_disk();

        if self.mode == Mode::Leet {
            let slug = self.leet.current().map(|p| p.title_slug.clone());
            match slug {
                Some(slug) if !self.leet.details.contains_key(&slug) => match &self.leet.want {
                    Some((w, since)) if *w == slug => {
                        if now - since > 0.25 {
                            self.leet.details.insert(slug.clone(), Detail::Loading);
                            self.leet.want = None;
                            jobs::leet_detail(self.tx.clone(), slug);
                        }
                    }
                    _ => self.leet.want = Some((slug, now)),
                },
                _ => self.leet.want = None,
            }
        }
    }

    /// Open `path` in $VISUAL / $EDITOR once the loop can hand over the terminal.
    pub fn edit(&mut self, path: PathBuf) {
        if path.exists() {
            self.pending_edit = Some(path);
        } else {
            self.toast(ToastKind::Err, format!("{} does not exist", path.display()));
        }
    }
}
