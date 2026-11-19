//! The LeetCode list's right-hand side: a preview of the highlighted problem
//! and what Enter will do.

use crate::tui::paint::*;
use crate::tui::state::*;

impl Cv<'_> {
    pub(super) fn leet_preview(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let leet = &app.leet;
        match &leet.list {
            LeetList::Unloaded | LeetList::Loading => {
                let x = self.squares(g.cx, g.top);
                self.put(x + 1, g.top, "Loading the problem list", st(th.dim));
                return;
            }
            LeetList::Failed(e) => {
                self.put(g.cx, g.top, "Could not load the problem list", bold(th.err));
                for (i, l) in e.lines().take(4).enumerate() {
                    self.put(
                        g.cx,
                        g.top + 1 + i as u16,
                        &trunc(l, g.text_w()),
                        st(th.dim),
                    );
                }
                let x = self.put(g.cx, g.top + 6, "ctrl+r", bold(th.fg));
                self.put(x + 1, g.top + 6, "tries again", st(th.dim));
                return;
            }
            LeetList::Ready(_) => {}
        }
        let Some(p) = leet.current() else {
            self.put(g.cx, g.top, "No problems match.", st(th.fg));
            let x = self.put(g.cx, g.top + 1, "esc", bold(th.fg));
            self.put(x + 1, g.top + 1, "clears the search", st(th.dim));
            return;
        };

        let tags: Vec<&str> = p.topic_tags.iter().map(|t| t.name.as_str()).collect();
        let mut y = g.top;
        if !tags.is_empty() {
            self.put(g.cx, y, &trunc(&tags.join(" · "), g.text_w()), st(th.dim));
            y += 2;
        }

        let action_y = g.bar_y.saturating_sub(5);
        let rows = action_y.saturating_sub(y + 1) as usize;
        match leet.details.get(&p.title_slug) {
            Some(Detail::Ready(text)) => {
                self.markdown(y, rows, &strip_title(text), 0);
            }
            Some(Detail::Failed(e)) => {
                self.put(
                    g.cx,
                    y,
                    &trunc(&format!("No preview: {e}"), g.text_w()),
                    st(th.dim),
                );
            }
            _ => {
                let x = self.squares(g.cx, y);
                self.put(x + 1, y, "Fetching the description", st(th.dim));
            }
        }

        if app.confirm.is_some() {
            return;
        }
        let folder = p.folder_name();
        let local = app
            .ws
            .find(&p.frontend_id)
            .is_some_and(|l| l.folder.is_some());
        if local {
            self.block(action_y, 4, th.ok);
            self.put(
                g.cx,
                action_y + 1,
                &trunc(&format!("Already pulled into {folder}/"), g.text_w()),
                bold(th.fg),
            );
            self.keys(
                g.cx,
                action_y + 2,
                &[
                    ("↵", "open it"),
                    ("^f", "re-scaffold (overwrites solution.cpp)"),
                ],
            );
        } else {
            self.block(action_y, 4, th.acc);
            self.put(
                g.cx,
                action_y + 1,
                &trunc(&format!("Pull into {folder}/"), g.text_w()),
                bold(th.fg),
            );
            self.put(
                g.cx,
                action_y + 2,
                "solution.cpp · testcases.txt · question.md",
                st(th.dim),
            );
        }
    }
}

/// Drop the leading "# Title" line; the title is already in the header.
pub fn strip_title(text: &str) -> String {
    if text.lines().any(|l| l.starts_with("# ")) {
        text.lines()
            .skip_while(|l| !l.starts_with("# "))
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text.to_string()
    }
}
