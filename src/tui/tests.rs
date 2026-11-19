//! Render the app into an in-memory terminal and check what is on screen.
//! Set `RAK_TUI_DUMP=1` and run with `--nocapture` to print the screens.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::state::*;
use super::theme::Theme;
use super::{data, ui};
use crate::judge::verdict::{CaseResult, Verdict};

struct Fixture {
    _dir: tempfile::TempDir,
    app: App,
    _rx: Receiver<Msg>,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("rak.toml"), "leetcode_dir = \"leetcode\"\n").unwrap();

    let today = chrono::Local::now().date_naive();
    let day = |n: i64| {
        (today - chrono::Duration::days(n))
            .format("%Y-%m-%d")
            .to_string()
    };
    let history = format!(
        "problems:\n\
         \x20 '54':\n    title: Spiral Matrix\n    difficulty: Medium\n    box_num: 2\n    streak_perfect: 0\n    last_review: {d3}\n    attempts:\n    - date: {d9}\n      rating: 2\n    - date: {d3}\n      rating: 4\n\
         \x20 '42':\n    title: Trapping Rain Water\n    difficulty: Hard\n    box_num: 1\n    streak_perfect: 0\n    last_review: {d3}\n    attempts:\n    - date: {d3}\n      rating: 2\n\
         \x20 '1':\n    title: Two Sum\n    difficulty: Easy\n    box_num: 5\n    streak_perfect: 3\n    last_review: {d1}\n    attempts:\n    - date: {d1}\n      rating: 5\n\
         \x20 '59':\n    title: Spiral Matrix II\n    difficulty: Medium\n    box_num: 1\n    streak_perfect: 0\n    last_review: {d1}\n    attempts: []\n",
        d1 = day(1),
        d3 = day(3),
        d9 = day(9),
    );
    std::fs::write(root.join("history.yaml"), history).unwrap();

    let folder = root.join("leetcode/0054.spiral-matrix");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("question.md"),
        "# [54. Spiral Matrix][link] (Medium)\n\n[link]: https://leetcode.com/problems/spiral-matrix/\n\n\
         Given an `m x n` matrix, return **all elements** of the matrix in spiral order.\n\n\
         ## Constraints\n\n- m == matrix.length\n- n == matrix[i].length\n",
    )
    .unwrap();
    std::fs::write(folder.join("solution.cpp"), "// solution\n").unwrap();
    std::fs::write(
        folder.join("testcases.txt"),
        "input:\n[[1,2],[3,4]]\noutput:\n[1,2,4,3]\ninput:\n[[7],[9],[6]]\noutput:\n[7,9,6]\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("analysis.md"),
        "## Approach\n\nYou walked the boundary with four pointers.\n",
    )
    .unwrap();

    let cfg = crate::config::load(root).unwrap();
    let ws = data::Workspace::load(root, &root.join("leetcode")).unwrap();
    let (tx, rx) = mpsc::channel();
    let app = App::new(ws, root.to_path_buf(), &cfg, tx);
    Fixture {
        _dir: dir,
        app,
        _rx: rx,
    }
}

fn screen(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app, Theme::detect())).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        let line: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    if std::env::var_os("RAK_TUI_DUMP").is_some() {
        eprintln!("{out}");
    }
    out
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn code(app: &mut App, k: KeyCode) {
    app.on_key(KeyEvent::new(k, KeyModifiers::NONE));
}

#[test]
fn today_lists_what_is_due() {
    let mut f = fixture();
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Today"), "{s}");
    assert!(s.contains("Due now"), "{s}");
    assert!(s.contains("Spiral Matrix"), "{s}");
    assert!(s.contains("Leitner boxes"), "{s}");
    // 59 was pulled but never attempted.
    assert!(s.contains("+ 59"), "{s}");
}

#[test]
fn selecting_a_problem_shows_its_question_and_tabs() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    for tab in [
        "Question", "Tests", "Record", "Analysis", "Submit", "History",
    ] {
        assert!(s.contains(tab), "missing {tab}\n{s}");
    }
    assert!(s.contains("return all elements"), "{s}");
    assert!(s.contains("Constraints"), "{s}");
    assert!(!s.contains("[link]"), "link reference leaked\n{s}");
    assert!(s.contains("Press : to type a command"), "{s}");
}

#[test]
fn keys_move_between_rows_and_tabs() {
    let mut f = fixture();
    key(&mut f.app, 'j');
    assert!(f.app.sel.is_some());
    let first = f.app.sel.clone();
    key(&mut f.app, 'j');
    assert_ne!(f.app.sel, first);
    code(&mut f.app, KeyCode::Enter);
    assert!(f.app.focus == Focus::Main);
    key(&mut f.app, ']');
    assert!(f.app.tab == Tab::Tests);
    key(&mut f.app, '[');
    key(&mut f.app, '[');
    assert!(f.app.tab == Tab::History);
    code(&mut f.app, KeyCode::Esc);
    assert!(f.app.focus == Focus::Rail);
    code(&mut f.app, KeyCode::Esc);
    assert!(f.app.sel.is_none(), "esc from the list goes back to Today");
}

#[test]
fn empty_tabs_say_which_key_fills_them() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tab = Tab::Tests;
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("No test run yet."), "{s}");
    assert!(s.contains("runs 2 cases"), "{s}");

    f.app.tab = Tab::Submit;
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Not submitted yet."), "{s}");
}

#[test]
fn analysis_tab_renders_markdown() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tab = Tab::Analysis;
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Approach"), "{s}");
    assert!(s.contains("four pointers"), "{s}");
    assert!(!s.contains("##"), "heading markers should be gone\n{s}");
}

#[test]
fn history_shows_the_ladder_attempts_and_rating() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tab = Tab::History;
    f.app.focus = Focus::Main;
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Leitner box"), "{s}");
    assert!(s.contains("★★☆☆☆"), "{s}");
    assert!(s.contains("Rate today"), "{s}");
    assert!(s.contains("Solved with minor hesitation"), "{s}");
    assert!(f.app.rating_visible());
}

#[test]
fn submitting_asks_first() {
    let mut f = fixture();
    f.app.select("54");
    key(&mut f.app, 'S');
    assert!(matches!(f.app.confirm, Some(Confirm::Submit(_))));
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("to LeetCode?"), "{s}");
    code(&mut f.app, KeyCode::Esc);
    assert!(f.app.confirm.is_none());
    assert!(matches!(
        f.app.session("54").map(|s| &s.submit),
        None | Some(SubmitRun::Idle)
    ));
}

#[test]
fn failing_case_shows_a_highlighted_diff() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tab = Tab::Tests;
    f.app.focus = Focus::Main;
    f.app.handle(Msg::TestBuilt(
        "54".into(),
        Duration::from_millis(1400),
        vec![1, 2],
    ));
    let row = |n, verdict| CaseRow {
        n,
        input: "matrix = [[7],[9],[6]]\n".into(),
        expected: Some("[7,9,6]".into()),
        result: CaseResult {
            verdict,
            debug: String::new(),
            stderr: String::new(),
        },
        elapsed: Duration::from_millis(2),
    };
    f.app
        .handle(Msg::TestCase("54".into(), row(1, Verdict::Accepted)));
    f.app.handle(Msg::TestCase(
        "54".into(),
        row(
            2,
            Verdict::WrongAnswer {
                expected: "[7,9,6]".into(),
                actual: "[7,9,6,9]".into(),
            },
        ),
    ));
    f.app.handle(Msg::TestDone("54".into()));
    assert_eq!(
        f.app.session("54").unwrap().case_sel,
        1,
        "the failing case is selected"
    );
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("1 passed"), "{s}");
    assert!(s.contains("1 failed"), "{s}");
    assert!(s.contains("Wrong Answer"), "{s}");
    assert!(s.contains("[7,9,6,9]"), "{s}");
}

#[test]
fn palette_suggests_and_runs_commands() {
    let mut f = fixture();
    f.app.select("54");
    key(&mut f.app, ':');
    for c in "rec".chars() {
        key(&mut f.app, c);
    }
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("record 54"), "{s}");
    assert!(!s.contains("pull "), "suggestions are filtered\n{s}");
    code(&mut f.app, KeyCode::Esc);
    assert!(matches!(f.app.overlay, Overlay::None));

    key(&mut f.app, '?');
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Command palette"), "{s}");
}

#[test]
fn leet_mode_opens_a_search_list() {
    let mut f = fixture();
    // Skip the network: pretend the list loaded.
    f.app.mode = Mode::Leet;
    f.app.leet.list = LeetList::Ready(vec![
        serde_json::from_str(
            r#"{"questionFrontendId":"54","titleSlug":"spiral-matrix","title":"Spiral Matrix","difficulty":"Medium","topicTags":[{"slug":"array","name":"Array"}],"status":null}"#,
        )
        .unwrap(),
        serde_json::from_str(
            r#"{"questionFrontendId":"885","titleSlug":"spiral-matrix-iii","title":"Spiral Matrix III","difficulty":"Medium","topicTags":[],"status":"ac"}"#,
        )
        .unwrap(),
    ]);
    f.app.leet.refilter();
    for c in "iii".chars() {
        key(&mut f.app, c);
    }
    assert_eq!(f.app.leet.filtered.len(), 1);
    f.app.leet.details.insert(
        "spiral-matrix-iii".into(),
        Detail::Ready("# 885. Spiral Matrix III\n\nStart at a cell and walk clockwise.".into()),
    );
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Spiral Matrix III"), "{s}");
    assert!(s.contains("walk clockwise"), "{s}");
    assert!(s.contains("solved on LeetCode"), "{s}");
    assert!(s.contains("Pull into 0885.spiral-matrix-iii/"), "{s}");
}

#[test]
fn narrow_and_tiny_terminals_degrade() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tick();
    let narrow = screen(&mut f.app, 80, 26);
    assert!(narrow.contains("Question"), "{narrow}");
    assert!(
        !narrow.contains("tracked"),
        "narrow sidebar drops the status text\n{narrow}"
    );
    let tiny = screen(&mut f.app, 50, 12);
    assert!(tiny.contains("Terminal too small"), "{tiny}");
    // A tall, wide terminal still draws.
    let big = screen(&mut f.app, 200, 60);
    assert!(big.contains("Spiral Matrix"), "{big}");
}

#[test]
fn route_opens_the_right_screen() {
    let mut f = fixture();
    f.app.apply_route(Route::Next);
    assert!(f.app.mode == Mode::Due);
    assert!(f.app.tab == Tab::History);
    assert!(f.app.sel.is_some());

    let mut f = fixture();
    f.app.apply_route(Route::Pull { refresh: false });
    assert!(f.app.mode == Mode::Leet);

    let mut f = fixture();
    f.app.apply_route(Route::Analyze {
        id: "54".into(),
        provider: None,
        force: false,
    });
    assert!(f.app.tab == Tab::Analysis);
    assert!(
        f.app.toast.is_some(),
        "an existing analysis is shown, not re-run"
    );

    let mut f = fixture();
    f.app.apply_route(Route::Test {
        id: "999".into(),
        case: None,
        timeout_ms: None,
        lang: None,
        debug: false,
    });
    assert!(f.app.toast.is_some(), "unknown problems explain themselves");
    assert!(f.app.sel.is_none());
}

fn accepted_result() -> crate::leetcode::models::CheckResult {
    serde_json::from_str(
        r#"{"state":"SUCCESS","status_msg":"Accepted","run_success":true,"total_correct":23,"total_testcases":23,
            "status_runtime":"0 ms","status_memory":"8.4 MB","runtime_percentile":100.0,"memory_percentile":62.13}"#,
    )
    .unwrap()
}

#[test]
fn every_screen_survives_every_size() {
    let mut f = fixture();
    // Give the selected problem some of everything.
    f.app.select("54");
    f.app.tick();
    f.app.handle(Msg::TestBuilt(
        "54".into(),
        Duration::from_millis(900),
        vec![1, 2],
    ));
    for n in 1..=2 {
        f.app.handle(Msg::TestCase(
            "54".into(),
            CaseRow {
                n,
                input: "[[1,2],[3,4]]\n".into(),
                expected: Some("[1,2,4,3]".into()),
                result: CaseResult {
                    verdict: if n == 1 {
                        Verdict::Accepted
                    } else {
                        Verdict::RuntimeError {
                            reason: "AddressSanitizer: heap-buffer-overflow\non line 12".into(),
                        }
                    },
                    debug: "debug line".into(),
                    stderr: "stderr line".into(),
                },
                elapsed: Duration::from_millis(3),
            },
        ));
    }
    f.app.handle(Msg::TestDone("54".into()));
    f.app
        .handle(Msg::SubmitDone("54".into(), Ok(accepted_result())));
    f.app.session_mut("54").pipe = Some(Pipe {
        attempt: 2,
        len: Duration::from_secs(252),
        stage: PipeStage::Done,
    });

    let sizes = [
        (60, 24),
        (64, 24),
        (80, 24),
        (80, 30),
        (100, 30),
        (100, 24),
        (120, 40),
        (200, 60),
    ];
    for (w, h) in sizes {
        for tab in super::state::TABS {
            f.app.tab = tab;
            for focus in [Focus::Rail, Focus::Main] {
                f.app.focus = focus;
                screen(&mut f.app, w, h);
                f.app.confirm = Some(Confirm::Submit("54".into()));
                screen(&mut f.app, w, h);
                f.app.confirm = None;
                f.app.overlay = Overlay::Palette {
                    input: "re".into(),
                    idx: 3,
                };
                screen(&mut f.app, w, h);
                f.app.overlay = Overlay::Help;
                screen(&mut f.app, w, h);
                f.app.overlay = Overlay::None;
            }
        }
        f.app.sel = None;
        screen(&mut f.app, w, h);
        f.app.mode = Mode::Leet;
        for list in [
            LeetList::Loading,
            LeetList::Failed("HTTP 403".into()),
            LeetList::Ready(vec![]),
        ] {
            f.app.leet.list = list;
            f.app.leet.refilter();
            screen(&mut f.app, w, h);
        }
        f.app.mode = Mode::All;
        f.app.select("54");
    }
}

#[test]
fn a_submission_result_shows_percentiles_and_asks_for_a_rating() {
    let mut f = fixture();
    f.app.select("54");
    f.app.session_mut("54").submit = SubmitRun::Done(Box::new(accepted_result()));
    f.app.tab = Tab::Submit;
    f.app.focus = Focus::Main;
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    assert!(s.contains("Accepted"), "{s}");
    assert!(s.contains("23/23 testcases"), "{s}");
    assert!(s.contains("beats 100.0%"), "{s}");
    assert!(s.contains("beats 62.1%"), "{s}");
    assert!(s.contains("Rate this attempt"), "{s}");
    assert!(f.app.rating_visible());
}

#[test]
fn markdown_in_questions_keeps_code_spans_attached_to_punctuation() {
    let mut f = fixture();
    f.app.select("54");
    f.app.tick();
    let s = screen(&mut f.app, 100, 30);
    assert!(
        s.contains("Given an m x n matrix, return all elements"),
        "{s}"
    );
}
