//! Work that must not block the UI: building and running tests, the
//! transcribe-then-analyze pipeline, submitting, and talking to LeetCode.
//! Each job runs on its own thread and reports back with a `Msg`.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Instant;

use super::state::{CaseRow, Msg, PipeStage};
use crate::commands::{analyze, pull, push, record, test};
use crate::config;
use crate::leetcode::client::LeetcodeClient;
use crate::leetcode::credentials;

fn cwd() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| e.to_string())
}

pub struct TestArgs {
    pub case: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub lang: Option<String>,
    pub debug: bool,
}

pub fn test(tx: Sender<Msg>, id: String, args: TestArgs) {
    thread::spawn(move || {
        let send = |m: Msg| {
            let _ = tx.send(m);
        };
        let plan = match test::plan(&id, args.case, args.timeout_ms, args.lang, args.debug) {
            Ok(p) => p,
            Err(e) => return send(Msg::TestFailed(id, e)),
        };
        let started = Instant::now();
        let exe = match plan.build() {
            Ok(e) => e,
            Err(e) => return send(Msg::TestFailed(id, e.to_string())),
        };
        let numbers = plan.cases.iter().map(|(n, _)| *n).collect();
        send(Msg::TestBuilt(id.clone(), started.elapsed(), numbers));
        for (n, case) in &plan.cases {
            match plan.run_case(&exe, case) {
                Ok((result, elapsed)) => send(Msg::TestCase(
                    id.clone(),
                    CaseRow {
                        n: *n,
                        input: case.input.clone(),
                        expected: case.expected.clone(),
                        result,
                        elapsed,
                    },
                )),
                Err(e) => return send(Msg::TestFailed(id, e)),
            }
        }
        send(Msg::TestDone(id));
    });
}

/// After a recording: transcribe it, then analyze the problem.
pub fn pipeline(tx: Sender<Msg>, id: String, audio_path: String, attempt: usize) {
    thread::spawn(move || {
        let send = |stage: PipeStage| {
            let _ = tx.send(Msg::Pipe(id.clone(), stage));
        };
        let setup = || -> Result<_, String> {
            let dir = cwd()?;
            let cfg = config::load(&dir)?;
            let problem_dir =
                config::resolve_problem_folder(&dir, &cfg, &config::ProblemId::parse(&id))?;
            Ok((cfg, problem_dir))
        };
        let (cfg, problem_dir) = match setup() {
            Ok(v) => v,
            Err(e) => return send(PipeStage::Failed(e)),
        };
        match record::transcribe_attempt(&cfg, &problem_dir, &audio_path, attempt) {
            Ok(true) => {}
            Ok(false) => return send(PipeStage::Failed("the transcript came back empty".into())),
            Err(e) => return send(PipeStage::Failed(format!("transcription failed: {e}"))),
        }
        send(PipeStage::Analyzing);
        match analyze::run(id.clone(), None, true) {
            Ok(()) => send(PipeStage::Done),
            Err(e) => send(PipeStage::Failed(format!("analysis failed: {e}"))),
        }
    });
}

pub fn analyze(tx: Sender<Msg>, id: String, provider: Option<String>, force: bool) {
    thread::spawn(move || {
        let r = analyze::run(id.clone(), provider, force);
        let _ = tx.send(Msg::AnalyzeDone(id, r));
    });
}

pub fn submit(tx: Sender<Msg>, id: String) {
    thread::spawn(move || {
        let stage_tx = tx.clone();
        let stage_id = id.clone();
        let result = push::submit(&id, &mut |stage| {
            let _ = stage_tx.send(Msg::SubmitStage(stage_id.clone(), stage.to_string()));
        });
        let _ = tx.send(Msg::SubmitDone(id, result));
    });
}

/// Load the LeetCode problem list from the cache, refreshing it when stale.
pub fn leet_list(tx: Sender<Msg>, refresh: bool) {
    thread::spawn(move || {
        let load = || -> Result<_, String> {
            let dir = cwd()?;
            config::load(&dir)?;
            let rak_toml_dir = config::find_rak_toml(&dir)?
                .parent()
                .ok_or("rak.toml has no parent directory")?
                .to_path_buf();
            let client = LeetcodeClient::new(credentials::resolve().ok())?;
            pull::load_or_refresh_cache(&client, &rak_toml_dir, refresh)
        };
        let _ = tx.send(Msg::LeetList(load()));
    });
}

pub fn leet_detail(tx: Sender<Msg>, slug: String) {
    thread::spawn(move || {
        let load = || -> Result<String, String> {
            let client = LeetcodeClient::new(credentials::resolve().ok())?;
            let detail = client.fetch_question_detail(&slug)?;
            if detail.is_paid_only && detail.content.is_none() {
                return Err("Premium-only problem".to_string());
            }
            Ok(crate::scaffold::preview_text(&detail))
        };
        let result = load();
        let _ = tx.send(Msg::LeetDetail(slug, result));
    });
}

/// Pull a problem by number, slug, or "today".
pub fn pull(tx: Sender<Msg>, target: String, force: bool, refresh: bool, label: String) {
    thread::spawn(move || {
        let run = || -> Result<pull::Scaffolded, String> {
            let dir = cwd()?;
            let cfg = config::load(&dir)?;
            let rak_toml_dir = config::find_rak_toml(&dir)?
                .parent()
                .ok_or("rak.toml has no parent directory")?
                .to_path_buf();
            let client = LeetcodeClient::new(credentials::resolve().ok())?;
            let slug = if target == "today" {
                client.fetch_daily_slug()?
            } else if let Ok(n) = target.parse::<u32>() {
                pull::resolve_by_id(n, &client, &rak_toml_dir, refresh)?.title_slug
            } else {
                target
            };
            pull::scaffold_slug(&client, &cfg, &rak_toml_dir, &slug, force)
        };
        let _ = tx.send(Msg::Pulled {
            label,
            result: run(),
        });
    });
}
