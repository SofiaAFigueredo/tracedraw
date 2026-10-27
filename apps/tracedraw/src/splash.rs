//! The splash screen. While TraceDraw starts, a small borderless window
//! centred on the screen shows the application icon, the name, the
//! tagline and the version over the brand artwork (see `ui/brand.rs`),
//! with what is loading and a progress bar. The splash stays up only as
//! long as the start takes: nothing holds it back once the editor can be
//! built (opening the file named on the command line), and the window
//! becomes the main window, maximized or at the size it was left. The
//! system fonts are read on a worker thread meanwhile (since the start of
//! the program, see `main.rs`); the splash waits for them only when the
//! interface language needs one, the editor takes them in when they arrive
//! otherwise. Options > General turns the splash off; `--no-splash` or
//! `TRACEDRAW_NO_SPLASH=1` skip it for one run.

use crate::i18n::{tr, trf};
use crate::settings::Settings;
use crate::theme::FontLoad;
use crate::ui::brand;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Stroke, TextureHandle, Vec2};
use std::path::Path;
use web_time::{Duration, Instant};

/// The splash window's size in points.
pub const SIZE: Vec2 = Vec2::new(640.0, 380.0);
/// The main window's size when none was kept, and its minimum.
pub const DEFAULT_SIZE: Vec2 = Vec2::new(1600.0, 950.0);
/// Small enough for a 1280 x 720 screen with a taskbar, at any interface
/// size (see [`min_size_for`]).
pub const MIN_SIZE: Vec2 = Vec2::new(800.0, 500.0);
/// How fast the progress bar eases towards [`WAITING_FILL`] while the
/// splash waits for the fonts of the interface language.
const EASE_TIME: Duration = Duration::from_millis(1600);
/// How far the bar goes while the splash waits for something.
const WAITING_FILL: f32 = 0.8;

/// What the shell does after a splash frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Keep showing the splash.
    Wait,
    /// Build the editor and open the main window.
    Start,
}

/// The splash screen's state.
pub struct Splash {
    started: Instant,
    /// The interface fonts, read on a worker thread.
    fonts: FontLoad,
    /// The interface language needs a system font: its texts wait for the
    /// fonts, and so does the start of the editor.
    wait_for_fonts: bool,
    /// Frames drawn since the full fonts were installed (they apply from
    /// the next frame on).
    frames_with_fonts: Option<u32>,
    /// The file named on the command line, shown while it opens.
    file_name: Option<String>,
    /// True once the last frame (the "opening" status, a full bar) shows.
    last_frame_drawn: bool,
    icon: Option<TextureHandle>,
    /// F8 was held while the splash showed: the editor asks whether to
    /// reset every preference.
    pub reset_requested: bool,
}

impl Splash {
    /// Start the splash: the bundled fonts at once, the system ones on a
    /// worker thread.
    pub fn start(ctx: &egui::Context, file: Option<&Path>) -> Self {
        ctx.set_fonts(crate::theme::bundled_font_definitions());
        ctx.set_visuals(crate::theme::visuals());
        Splash {
            started: Instant::now(),
            fonts: FontLoad::start(),
            wait_for_fonts: crate::theme::needs_system_font(&crate::i18n::language()),
            frames_with_fonts: None,
            file_name: file
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            last_frame_drawn: false,
            icon: brand::icon(ctx),
            reset_requested: false,
        }
    }

    /// Draw one splash frame in `ui` (the whole window); returns
    /// [`Step::Start`] when the editor should be built.
    pub fn update(&mut self, ui: &mut egui::Ui) -> Step {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_down(egui::Key::F8)) {
            self.reset_requested = true;
        }
        if self.frames_with_fonts.is_none() && self.fonts.is_finished() {
            ctx.set_fonts(self.fonts.finish());
            self.frames_with_fonts = Some(0);
        } else if let Some(n) = self.frames_with_fonts.as_mut() {
            *n = n.saturating_add(1);
        }
        let elapsed = self.started.elapsed();
        // The editor starts without the system fonts unless its texts
        // need one; they are installed when they arrive.
        let fonts_ready = self.frames_with_fonts.is_some() || !self.wait_for_fonts;
        let texts = !self.wait_for_fonts || self.frames_with_fonts.is_some_and(|n| n >= 1);
        let status = status_text(fonts_ready, self.file_name.as_deref());
        paint(
            ui,
            progress(elapsed, fonts_ready),
            texts.then_some(status.as_str()),
            texts,
            self.icon.as_ref(),
        );
        if fonts_ready {
            if self.last_frame_drawn {
                return Step::Start;
            }
            self.last_frame_drawn = true;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        Step::Wait
    }

    /// The fonts still being read when the editor starts: the shell
    /// installs them when they are in.
    pub fn take_fonts(&mut self) -> FontLoad {
        std::mem::take(&mut self.fonts)
    }
}

/// The status line: the fonts while they load, then the file being
/// opened or the workspace.
pub fn status_text(fonts_ready: bool, file: Option<&str>) -> String {
    match (fonts_ready, file) {
        (false, _) => tr("splash.loading_fonts"),
        (true, Some(name)) => trf("splash.opening", &[("name", name)]),
        (true, None) => tr("splash.preparing"),
    }
}

/// The progress bar's fill, 0 to 1: it eases towards 80 % over
/// [`EASE_TIME`] while the fonts load and is full once they are in (the
/// editor is built right after).
pub fn progress(elapsed: Duration, fonts_ready: bool) -> f32 {
    if fonts_ready {
        return 1.0;
    }
    let t = (elapsed.as_secs_f32() / EASE_TIME.as_secs_f32()).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - t) * (1.0 - t);
    WAITING_FILL * eased
}

/// Paint the splash over the whole of `ui`. `texts` draws the tagline and
/// the copyright line (left out while the fonts for the language load).
fn paint(
    ui: &mut egui::Ui,
    progress: f32,
    status: Option<&str>,
    texts: bool,
    icon: Option<&TextureHandle>,
) {
    let rect = ui.max_rect();
    let painter = ui.painter_at(rect);
    brand::gradient(&painter, rect);
    let s = (rect.width() / SIZE.x).min(rect.height() / SIZE.y).max(0.5);
    brand::curves(&painter, rect, 1.35 * s, &[0.24, 0.62]);

    let pad = 44.0 * s;
    let icon_rect = Rect::from_min_size(rect.min + Vec2::new(pad, 64.0 * s), Vec2::splat(96.0 * s));
    brand::paint_icon(&painter, icon, icon_rect);
    let name = brand::name_galley(&painter, 46.0 * s);
    let tagline = texts.then(|| {
        painter.layout_no_wrap(
            tr("dialog.about_tagline"),
            FontId::proportional(17.0 * s),
            brand::TAGLINE,
        )
    });
    let block_h = name.size().y + tagline.as_ref().map_or(0.0, |g| g.size().y + 2.0 * s);
    let left = icon_rect.right() + 24.0 * s;
    let mut y = icon_rect.center().y - block_h / 2.0;
    let name_h = name.size().y;
    painter.galley(Pos2::new(left, y), name, Color32::WHITE);
    y += name_h + 2.0 * s;
    if let Some(g) = tagline {
        painter.galley(Pos2::new(left + 2.0 * s, y), g, brand::TAGLINE);
    }

    // A darker strip along the bottom for the status, the progress bar
    // and the footer, over the ends of the curves.
    let bar_y = rect.bottom() - 54.0 * s;
    let strip = Rect::from_min_max(
        Pos2::new(rect.left(), bar_y - 40.0 * s),
        rect.right_bottom(),
    );
    painter.rect_filled(
        strip,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(0x0A, 0x12, 0x22, 225),
    );
    painter.hline(
        strip.x_range(),
        strip.top(),
        Stroke::new(1.0, Color32::from_white_alpha(22)),
    );

    // The progress bar with the status above it.
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + pad, bar_y),
        Pos2::new(rect.right() - pad, bar_y + 3.0 * s),
    );
    painter.rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(30));
    let mut fill = track;
    fill.set_width(track.width() * progress.clamp(0.0, 1.0));
    painter.rect_filled(fill, CornerRadius::same(2), brand::CURVE);
    if let Some(text) = status {
        let g = painter.layout_no_wrap(
            text.to_string(),
            FontId::proportional(13.0 * s),
            brand::TAGLINE,
        );
        let h = g.size().y;
        painter.galley(
            Pos2::new(track.left(), bar_y - 8.0 * s - h),
            g,
            brand::TAGLINE,
        );
    }

    // The footer: copyright on the left, version on the right.
    let foot_y = rect.bottom() - 34.0 * s;
    let small = FontId::proportional(11.5 * s);
    if texts {
        let g = painter.layout_no_wrap(
            trf(
                "dialog.about_copyright",
                &[("name", env!("CARGO_PKG_AUTHORS"))],
            ),
            small.clone(),
            brand::FOOTNOTE,
        );
        painter.galley(Pos2::new(track.left(), foot_y), g, brand::FOOTNOTE);
    }
    let v = painter.layout_no_wrap(
        format!("v{}", env!("CARGO_PKG_VERSION")),
        small,
        brand::FOOTNOTE,
    );
    let vw = v.size().x;
    painter.galley(Pos2::new(track.right() - vw, foot_y), v, brand::FOOTNOTE);

    painter.rect_stroke(
        rect.shrink(0.5),
        CornerRadius::ZERO,
        Stroke::new(1.0, Color32::from_white_alpha(40)),
        egui::StrokeKind::Inside,
    );
}

/// The main window's size: the one kept from the last session when it
/// is sensible, else the default; never larger than the monitor.
pub fn main_window_size(saved: [f32; 2], monitor: Option<Vec2>) -> Vec2 {
    let saved = Vec2::new(saved[0], saved[1]);
    let mut size = if saved.x.is_finite()
        && saved.y.is_finite()
        && saved.x >= MIN_SIZE.x
        && saved.y >= MIN_SIZE.y
    {
        saved
    } else {
        DEFAULT_SIZE
    };
    if let Some(m) = monitor.filter(|m| m.x > 0.0 && m.y > 0.0) {
        size = size.min(m * 0.95).max(min_size_for(monitor));
    }
    size
}

/// The window's minimum size on a monitor of `monitor` (both in points):
/// [`MIN_SIZE`], never more than four fifths of the monitor, so the
/// window always fits the screen with room for a taskbar.
pub fn min_size_for(monitor: Option<Vec2>) -> Vec2 {
    match monitor.filter(|m| m.x > 0.0 && m.y > 0.0) {
        Some(m) => MIN_SIZE.min(m * 0.8),
        None => MIN_SIZE,
    }
}

/// The main window starts maximized when it was left so, or when the
/// size it would get does not fit the monitor (a small screen).
pub fn start_maximized(saved: [f32; 2], maximized: bool, monitor: Option<Vec2>) -> bool {
    if maximized {
        return true;
    }
    let Some(m) = monitor.filter(|m| m.x > 0.0 && m.y > 0.0) else {
        return false;
    };
    let saved = Vec2::new(saved[0], saved[1]);
    let wanted = if saved.x.is_finite()
        && saved.y.is_finite()
        && saved.x >= MIN_SIZE.x
        && saved.y >= MIN_SIZE.y
    {
        saved
    } else {
        DEFAULT_SIZE
    };
    wanted.x > m.x * 0.95 || wanted.y > m.y * 0.95
}

/// Where a window of `size` sits centred on a monitor of `monitor` (its
/// top-left corner, never off the top or the left).
pub fn centred_position(size: Vec2, monitor: Option<Vec2>) -> Option<Pos2> {
    let m = monitor.filter(|m| m.x > 0.0 && m.y > 0.0)?;
    let p = ((m - size) / 2.0).max(Vec2::ZERO);
    Some(Pos2::new(p.x, p.y))
}

/// Turn the splash window into the main window: decorations, resizable,
/// the kept size centred on the monitor, maximized when it was left so.
pub fn open_main_window(ctx: &egui::Context, settings: &Settings) {
    use egui::ViewportCommand as V;
    let monitor = ctx.input(|i| i.viewport().monitor_size);
    let size = main_window_size(settings.window_size, monitor);
    ctx.send_viewport_cmd(V::Decorations(true));
    // The splash window is made not resizable, which on X11 records its
    // own size as the window's minimum and maximum; making it resizable
    // again restores those. Clear the maximum and set the minimum first,
    // or the main window keeps the splash's maximum and cannot be
    // maximized or resized.
    ctx.send_viewport_cmd(V::MaxInnerSize(egui::Vec2::INFINITY));
    ctx.send_viewport_cmd(V::MinInnerSize(min_size_for(monitor)));
    ctx.send_viewport_cmd(V::Resizable(true));
    ctx.send_viewport_cmd(V::EnableButtons {
        close: true,
        minimized: true,
        maximize: true,
    });
    ctx.send_viewport_cmd(V::InnerSize(size));
    if let Some(p) = centred_position(size, monitor) {
        ctx.send_viewport_cmd(V::OuterPosition(p));
    }
    if start_maximized(settings.window_size, settings.window_maximized, monitor) {
        ctx.send_viewport_cmd(V::Maximized(true));
    }
    ctx.send_viewport_cmd(V::Focus);
}

/// Keep the main window's state for the next start: maximized or not,
/// and the size while it is neither maximized nor minimized.
pub fn remember_window(ctx: &egui::Context, settings: &mut Settings) {
    let (maximized, minimized, inner) = ctx.input(|i| {
        let v = i.viewport();
        (v.maximized, v.minimized, v.inner_rect)
    });
    if minimized == Some(true) {
        return;
    }
    if let Some(m) = maximized {
        settings.window_maximized = m;
        if !m {
            if let Some(r) = inner.filter(|r| r.width() >= MIN_SIZE.x && r.height() >= MIN_SIZE.y) {
                settings.window_size = [r.width(), r.height()];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_window_drops_the_splash_maximum_before_it_turns_resizable() {
        use egui::ViewportCommand as V;
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            open_main_window(ui.ctx(), &Settings::default());
        });
        // No painter takes the textures here.
        out.textures_delta.clear();
        let cmds = &out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("the root viewport")
            .commands;
        let at = |want: &dyn Fn(&V) -> bool| cmds.iter().position(want);
        let max = at(&|c| matches!(c, V::MaxInnerSize(s) if s == &egui::Vec2::INFINITY))
            .expect("the maximum is cleared");
        let min =
            at(&|c| matches!(c, V::MinInnerSize(s) if s == &MIN_SIZE)).expect("the minimum is set");
        let resizable =
            at(&|c| matches!(c, V::Resizable(true))).expect("the window turns resizable");
        assert!(max < resizable && min < resizable, "{cmds:?}");
    }

    #[test]
    fn small_screens_get_a_window_that_fits_and_starts_maximized() {
        // 1366 x 768 and 1280 x 720 screens, at 100 % and at 125 % (the
        // monitor's size in points shrinks as the interface grows).
        for m in [
            Vec2::new(1366.0, 768.0),
            Vec2::new(1280.0, 720.0),
            Vec2::new(1366.0, 768.0) / 1.25,
            Vec2::new(1280.0, 720.0) / 1.25,
        ] {
            let min = min_size_for(Some(m));
            assert!(
                min.x <= m.x * 0.8 + 0.01 && min.y <= m.y * 0.8 + 0.01,
                "{m:?} {min:?}"
            );
            let size = main_window_size([0.0, 0.0], Some(m));
            assert!(size.x <= m.x && size.y <= m.y, "{m:?} {size:?}");
            assert!(size.x >= min.x && size.y >= min.y);
            // The default size does not fit: the window starts maximized.
            assert!(start_maximized([0.0, 0.0], false, Some(m)));
        }
        // A large screen keeps the kept size, not maximized.
        let big = Some(Vec2::new(2560.0, 1440.0));
        assert!(!start_maximized([1600.0, 950.0], false, big));
        assert!(start_maximized([1600.0, 950.0], true, big));
        assert_eq!(min_size_for(big), MIN_SIZE);
    }

    #[test]
    fn the_main_window_offers_the_maximize_button() {
        use egui::ViewportCommand as V;
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            open_main_window(ui.ctx(), &Settings::default());
        });
        out.textures_delta.clear();
        let cmds = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(cmds
            .iter()
            .any(|c| matches!(c, V::EnableButtons { maximize: true, .. })));
        assert!(cmds.iter().any(|c| matches!(c, V::Decorations(true))));
    }

    #[test]
    fn progress_waits_for_the_fonts_and_holds_nothing_back() {
        assert_eq!(progress(Duration::ZERO, false), 0.0);
        let half = progress(EASE_TIME / 2, false);
        assert!(half > 0.4 && half < 0.8, "{half}");
        // Without the fonts the bar stops at 80 %.
        assert!((progress(EASE_TIME * 3, false) - WAITING_FILL).abs() < 1e-6);
        // With them it is full at once: no minimum time keeps the splash.
        assert!((progress(Duration::ZERO, true) - 1.0).abs() < 1e-6);
        assert!((progress(EASE_TIME / 2, true) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_status_names_the_file_being_opened() {
        crate::i18n::set_language("en");
        assert_eq!(status_text(false, Some("a.cdr")), "Loading fonts...");
        assert_eq!(status_text(true, Some("a.cdr")), "Opening a.cdr...");
        assert_eq!(status_text(true, None), "Preparing the workspace...");
    }

    #[test]
    fn the_main_window_keeps_a_sensible_size() {
        let monitor = Some(Vec2::new(1920.0, 1080.0));
        // Nothing kept yet: the default size.
        assert_eq!(main_window_size([0.0, 0.0], monitor), DEFAULT_SIZE);
        // A kept size comes back.
        assert_eq!(
            main_window_size([1200.0, 800.0], monitor),
            Vec2::new(1200.0, 800.0)
        );
        // Never larger than the monitor (a 125 % laptop screen).
        let small = Some(Vec2::new(1536.0, 864.0));
        let s = main_window_size([0.0, 0.0], small);
        assert!(s.x <= 1536.0 && s.y <= 864.0 && s.x >= MIN_SIZE.x);
        // Nonsense falls back to the default.
        assert_eq!(main_window_size([f32::NAN, 5.0], None), DEFAULT_SIZE);
    }

    #[test]
    fn the_main_window_is_centred() {
        let p = centred_position(Vec2::new(1600.0, 900.0), Some(Vec2::new(1920.0, 1080.0)));
        assert_eq!(p, Some(Pos2::new(160.0, 90.0)));
        // Larger than the monitor: pinned to the top left.
        let p = centred_position(Vec2::new(2000.0, 900.0), Some(Vec2::new(1920.0, 800.0)));
        assert_eq!(p, Some(Pos2::new(0.0, 0.0)));
        assert_eq!(centred_position(Vec2::new(10.0, 10.0), None), None);
    }

    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        }
    }

    /// Splash frames until the editor starts (at most `max`); the steps.
    fn run_frames(ctx: &egui::Context, splash: &mut Splash, max: usize) -> Vec<Step> {
        let mut steps = Vec::new();
        for _ in 0..max {
            let mut out = ctx.run_ui(input(), |ui| steps.push(splash.update(ui)));
            out.textures_delta.clear();
            if steps.last() == Some(&Step::Start) {
                break;
            }
        }
        steps
    }

    #[test]
    fn a_splash_frame_draws_and_then_starts_the_editor() {
        crate::i18n::set_language("en");
        let ctx = egui::Context::default();
        let mut splash = Splash::start(&ctx, Some(Path::new("/tmp/a.cdr")));
        // No minimum time: the editor starts as soon as the last status
        // has been drawn once, with the fonts in or not.
        let steps = run_frames(&ctx, &mut splash, 4);
        assert_eq!(steps, vec![Step::Wait, Step::Start]);
        assert!(!splash.reset_requested);
    }

    #[test]
    fn the_editor_does_not_wait_for_system_fonts_the_language_does_not_need() {
        crate::i18n::set_language("en");
        let ctx = egui::Context::default();
        let mut splash = Splash::start(&ctx, None);
        assert!(!splash.wait_for_fonts);
        // Whatever the font thread has done, the editor starts in two
        // frames, and the load it leaves is the shell's to install.
        let steps = run_frames(&ctx, &mut splash, 4);
        assert_eq!(steps, vec![Step::Wait, Step::Start]);
        let mut load = splash.take_fonts();
        let defs = load.finish();
        assert!(!load.pending());
        assert!(defs.families.contains_key(&egui::FontFamily::Proportional));
        // Nothing is left to install afterwards.
        assert!(!load.poll(&ctx));
    }

    #[test]
    fn a_language_that_needs_a_system_font_waits_for_it() {
        crate::i18n::set_language("en");
        let ctx = egui::Context::default();
        let mut splash = Splash::start(&ctx, None);
        splash.wait_for_fonts = true;
        let mut steps = Vec::new();
        for _ in 0..3000 {
            let mut out = ctx.run_ui(input(), |ui| steps.push(splash.update(ui)));
            out.textures_delta.clear();
            if steps.last() == Some(&Step::Start) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(steps.last(), Some(&Step::Start));
        // The fonts were installed first and nothing is left to hand over.
        assert!(splash.frames_with_fonts.is_some());
        assert!(!splash.take_fonts().pending());
    }

    #[test]
    fn f8_while_starting_asks_to_reset_the_settings() {
        let ctx = egui::Context::default();
        let mut splash = Splash::start(&ctx, None);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            events: vec![egui::Event::Key {
                key: egui::Key::F8,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            splash.update(ui);
        });
        out.textures_delta.clear();
        assert!(splash.reset_requested);
    }
}
