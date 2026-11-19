//! Today: what is due, the Leitner boxes, and recent attempts. Shown when no
//! problem is selected.

use super::{due_color, due_label};
use crate::tui::data::{self, Problem};
use crate::tui::paint::*;
use crate::tui::state::*;

impl Cv<'_> {
    pub(super) fn today(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let mut y = g.top;

        let mut due: Vec<&Problem> = app.ws.problems.iter().filter(|p| p.is_due()).collect();
        due.sort_by_key(|p| std::cmp::Reverse(p.due));
        self.section(y, "Due now");
        y += 1;
        if due.is_empty() {
            let next = app
                .ws
                .problems
                .iter()
                .filter(|p| p.due.is_some_and(|d| d < 0))
                .max_by_key(|p| p.due);
            let fresh = app.ws.problems.iter().find(|p| p.is_new());
            let msg = match (fresh, next) {
                (Some(p), _) => format!(
                    "Nothing due. {} {} is new: open it to start.",
                    p.id, p.title
                ),
                (None, Some(p)) => format!(
                    "Nothing due. Next up: {} {} {}.",
                    p.id,
                    p.title,
                    due_label(p.due)
                ),
                (None, None) => "Nothing due. Pull a problem with p to start.".to_string(),
            };
            self.put(g.cx, y, &trunc(&msg, g.text_w()), st(th.dim));
            y += 1;
        }
        let shown = due.len().min(4);
        for p in due.iter().take(shown) {
            self.put(g.cx, y, &pad(&p.id, 6), st(th.dim));
            let title_w = g.text_w().saturating_sub(24).min(34);
            self.put(g.cx + 6, y, &pad(&p.title, title_w), st(th.fg));
            let label = due_label(p.due);
            self.put(
                g.cr - width(&label) as u16,
                y,
                &label,
                st(due_color(&th, p.due)),
            );
            self.put(
                g.cr.saturating_sub(width(&label) as u16 + 9),
                y,
                &format!("box {}", p.box_num),
                st(th.dim),
            );
            self.hit(g.cx, y, g.cr - g.cx, Action::OpenDue(p.id.clone()));
            y += 1;
        }
        if due.len() > shown {
            self.put(
                g.cx,
                y,
                &format!("+{} more · n steps through them", due.len() - shown),
                st(th.faint),
            );
            y += 1;
        }

        y += 1;
        if self.fits(y + 5) {
            self.section(y, "Leitner boxes");
            y += 1;
            let tracked: Vec<&Problem> = app.ws.problems.iter().filter(|p| !p.is_new()).collect();
            let max = (1..=5u8)
                .map(|b| tracked.iter().filter(|p| p.box_num == b).count())
                .max()
                .unwrap_or(1)
                .max(1);
            for b in 1..=5u8 {
                let ids: Vec<&str> = tracked
                    .iter()
                    .filter(|p| p.box_num == b)
                    .map(|p| p.id.as_str())
                    .collect();
                self.put(g.cx, y, &format!("box {b}"), st(th.dim));
                self.hbar(g.cx + 7, y, 20, ids.len() as f64 / max as f64, th.acc);
                self.put(g.cx + 29, y, &ids.len().to_string(), bold(th.fg));
                self.put(g.cx + 32, y, &trunc(&ids.join(" "), 18), st(th.dim));
                let every = format!(
                    "every {} {}",
                    data::interval(b),
                    if b == 1 { "day" } else { "days" }
                );
                if g.text_w() > 66 {
                    self.put(g.cr - width(&every) as u16, y, &every, st(th.faint));
                }
                y += 1;
            }
        }

        y += 1;
        if self.fits(y + 1) {
            let mut recent: Vec<(chrono::NaiveDate, &str, u8, u8)> = Vec::new();
            for p in &app.ws.problems {
                let mut bx = 1u8;
                let mut streak = 0u8;
                for (i, a) in p.attempts.iter().enumerate() {
                    let (nb, ns) = data::outcome(bx, streak, a.rating, i == 0);
                    bx = nb;
                    streak = ns;
                    recent.push((a.date, p.id.as_str(), a.rating, nb));
                }
            }
            recent.sort_by_key(|r| std::cmp::Reverse(r.0));
            if !recent.is_empty() {
                self.section(y, "Recent");
                y += 1;
                for (date, id, rating, nb) in recent.iter().take(3) {
                    if !self.fits(y) {
                        break;
                    }
                    let when = if *date == app.ws.today {
                        "today".to_string()
                    } else {
                        date.format("%b %-d").to_string()
                    };
                    self.put(g.cx, y, &pad(&when, 8), st(th.faint));
                    self.put(g.cx + 8, y, "log", st(th.vio));
                    self.put(g.cx + 16, y, &pad(id, 6), st(th.dim));
                    self.put(
                        g.cx + 22,
                        y,
                        &format!("rated {} → box {nb}", rating),
                        st(th.dim),
                    );
                    y += 1;
                }
            }
        }
    }
}
