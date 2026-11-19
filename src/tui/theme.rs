//! Colors. A flat, near-black palette with one warm accent: surfaces are told
//! apart by tone, never by borders. A light variant is picked from
//! `RAK_THEME=light` or the terminal's `COLORFGBG`.

use ratatui::style::Color;

#[derive(Clone, Copy)]
pub struct Theme {
    /// Main surface.
    pub bg: Color,
    /// Sidebar and footer surface.
    pub panel: Color,
    /// Selected rows, blocks and the command bar.
    pub raised: Color,
    /// Overlays and toasts.
    pub elev: Color,
    pub line: Color,
    pub fg: Color,
    pub dim: Color,
    pub faint: Color,
    /// Focus and actions.
    pub acc: Color,
    /// Text on top of `acc`.
    pub ink: Color,
    pub ok: Color,
    pub err: Color,
    pub warn: Color,
    pub info: Color,
    pub vio: Color,
    pub cya: Color,
}

type Rgb = (u8, u8, u8);

struct Palette {
    bg: Rgb,
    panel: Rgb,
    raised: Rgb,
    elev: Rgb,
    line: Rgb,
    fg: Rgb,
    dim: Rgb,
    faint: Rgb,
    acc: Rgb,
    ink: Rgb,
    ok: Rgb,
    err: Rgb,
    warn: Rgb,
    info: Rgb,
    vio: Rgb,
    cya: Rgb,
}

const DARK: Palette = Palette {
    bg: (0x0c, 0x0c, 0x0d),
    panel: (0x14, 0x14, 0x15),
    raised: (0x1b, 0x1b, 0x1d),
    elev: (0x23, 0x23, 0x26),
    line: (0x23, 0x23, 0x26),
    fg: (0xec, 0xeb, 0xe8),
    dim: (0x8e, 0x8d, 0x93),
    faint: (0x55, 0x54, 0x5a),
    acc: (0xf7, 0xa9, 0x7a),
    ink: (0x1c, 0x12, 0x0b),
    ok: (0x82, 0xd3, 0x9d),
    err: (0xf2, 0x8b, 0x8b),
    warn: (0xec, 0xc9, 0x6b),
    info: (0x82, 0xa7, 0xf5),
    vio: (0xab, 0x8f, 0xf7),
    cya: (0x62, 0xcf, 0xc1),
};

const LIGHT: Palette = Palette {
    bg: (0xfb, 0xfa, 0xf8),
    panel: (0xf2, 0xf0, 0xec),
    raised: (0xe9, 0xe6, 0xe0),
    elev: (0xe2, 0xde, 0xd6),
    line: (0xe1, 0xdd, 0xd5),
    fg: (0x1d, 0x1c, 0x1a),
    dim: (0x6c, 0x69, 0x64),
    faint: (0xa9, 0xa5, 0x9e),
    acc: (0xc4, 0x62, 0x2d),
    ink: (0xff, 0xfa, 0xf5),
    ok: (0x2f, 0x8a, 0x4f),
    err: (0xc4, 0x3d, 0x3d),
    warn: (0x94, 0x6f, 0x0c),
    info: (0x3a, 0x63, 0xc4),
    vio: (0x75, 0x52, 0xd1),
    cya: (0x13, 0x85, 0x7a),
};

impl Theme {
    pub fn detect() -> Theme {
        let light = match std::env::var("RAK_THEME").ok().as_deref() {
            Some("light") => true,
            Some("dark") => false,
            _ => colorfgbg_is_light(),
        };
        let truecolor = matches!(
            std::env::var("COLORTERM").ok().as_deref(),
            Some("truecolor") | Some("24bit")
        );
        Theme::from_palette(if light { &LIGHT } else { &DARK }, truecolor)
    }

    fn from_palette(p: &Palette, truecolor: bool) -> Theme {
        let c = |rgb: Rgb| color(rgb, truecolor);
        Theme {
            bg: c(p.bg),
            panel: c(p.panel),
            raised: c(p.raised),
            elev: c(p.elev),
            line: c(p.line),
            fg: c(p.fg),
            dim: c(p.dim),
            faint: c(p.faint),
            acc: c(p.acc),
            ink: c(p.ink),
            ok: c(p.ok),
            err: c(p.err),
            warn: c(p.warn),
            info: c(p.info),
            vio: c(p.vio),
            cya: c(p.cya),
        }
    }

    /// `acc` faded toward the background, for the activity squares.
    pub fn acc_fade(&self, t: f64) -> Color {
        mix(self.acc, self.bg, t)
    }
}

fn colorfgbg_is_light() -> bool {
    // "fg;bg" with ANSI indexes; 7 and 15 are the light backgrounds.
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|v| v.rsplit(';').next().map(str::to_string))
        .and_then(|bg| bg.parse::<u8>().ok())
        .is_some_and(|bg| bg == 7 || bg == 15)
}

fn color(rgb: Rgb, truecolor: bool) -> Color {
    if truecolor {
        Color::Rgb(rgb.0, rgb.1, rgb.2)
    } else {
        Color::Indexed(rgb_to_256(rgb))
    }
}

/// Nearest entry of the xterm 256-color palette.
fn rgb_to_256((r, g, b): Rgb) -> u8 {
    let level = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            ((v as u16 - 35) / 40) as u8
        }
    };
    let (cr, cg, cb) = (level(r), level(g), level(b));
    let cube = 16 + 36 * cr + 6 * cg + cb;
    let avg = (r as u16 + g as u16 + b as u16) / 3;
    let gray_idx = if avg < 8 {
        0
    } else if avg > 238 {
        23
    } else {
        ((avg - 8) / 10) as u8
    };
    let gray = 232 + gray_idx;
    let steps = [0u16, 95, 135, 175, 215, 255];
    let dist = |a: (u16, u16, u16)| {
        let d = |x: u16, y: u16| (x as i32 - y as i32).pow(2);
        d(a.0, r as u16) + d(a.1, g as u16) + d(a.2, b as u16)
    };
    let cube_rgb = (steps[cr as usize], steps[cg as usize], steps[cb as usize]);
    let gv = 8 + 10 * gray_idx as u16;
    if dist((gv, gv, gv)) < dist(cube_rgb) {
        gray
    } else {
        cube
    }
}

/// `a` blended toward `b` (`t` = 0 gives `a`, 1 gives `b`).
fn mix(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
            Color::Rgb(m(ar, br), m(ag, bg), m(ab, bb))
        }
        // No truecolor: step down to the dim end instead of blending.
        _ => {
            if t > 0.5 {
                b
            } else {
                a
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grays_map_to_the_gray_ramp() {
        assert!((232..=255).contains(&rgb_to_256((0x14, 0x14, 0x15))));
        assert_eq!(rgb_to_256((0, 0, 0)), 16);
    }

    #[test]
    fn mix_blends_rgb() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 50);
        assert_eq!(mix(a, b, 0.5), Color::Rgb(50, 100, 25));
        assert_eq!(mix(a, b, 0.0), a);
    }
}
