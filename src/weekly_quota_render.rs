//! Deterministic software renderer for the weekly-quota floating widget.
//!
//! The renderer works at 4× resolution and downsamples once. This keeps the
//! Win32 window controller independent from visual details and gives the
//! layered window smooth, repeatable per-pixel alpha.

use std::{f32::consts::TAU, fs, path::PathBuf, sync::OnceLock};

use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle, VerticalAlign},
};

/// The visible artwork remains 116 px wide; the extra transparent padding lets
/// the soft shadow fade naturally instead of being clipped by the HWND bounds.
pub const OUTPUT_SIZE: usize = 132;
const SCALE: usize = 4;
const HIGH_SIZE: usize = OUTPUT_SIZE * SCALE;
const CONTENT_PADDING: f32 = 8.0;
const CENTER_X: f32 = 58.0 + CONTENT_PADDING;
const CENTER_Y: f32 = 56.0 + CONTENT_PADDING;
const RING_RADIUS: f32 = 51.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeeklyQuotaRenderModel {
    pub remaining_percent: Option<u8>,
    pub percent_label: String,
    pub title: String,
    pub refresh_label: String,
}

impl WeeklyQuotaRenderModel {
    pub fn preview(remaining_percent: u8) -> Self {
        Self {
            remaining_percent: Some(remaining_percent.min(100)),
            percent_label: format!("{}%", remaining_percent.min(100)),
            title: "周剩余额度".into(),
            refresh_label: "刷新 14:52".into(),
        }
    }

    pub fn empty() -> Self {
        Self {
            remaining_percent: None,
            percent_label: "--%".into(),
            title: "周剩余额度".into(),
            refresh_label: "刷新 --:--".into(),
        }
    }
}

pub fn render_weekly_quota(model: &WeeklyQuotaRenderModel) -> Vec<u8> {
    render_impl(model, true)
}

/// Native hosts paint labels with the system font without loading the entire CJK font into RAM.
pub fn render_weekly_quota_base(model: &WeeklyQuotaRenderModel) -> Vec<u8> {
    render_impl(model, false)
}

fn render_impl(model: &WeeklyQuotaRenderModel, labels: bool) -> Vec<u8> {
    let mut canvas = Canvas::new();

    draw_shadow(&mut canvas);
    draw_outer_ambient_glow(&mut canvas);
    draw_track(&mut canvas);
    if let Some(remaining) = model.remaining_percent {
        draw_available_glow(&mut canvas, remaining);
        draw_available_ring(&mut canvas, remaining);
        if remaining > 0 {
            draw_soft_disc(
                &mut canvas,
                CENTER_X,
                CENTER_Y - RING_RADIUS,
                3.6,
                available_color(0.0),
                1.0,
            );
        }
    }
    draw_inner_disc(&mut canvas);
    draw_inner_highlight(&mut canvas);
    if let Some(remaining) = model.remaining_percent {
        draw_endpoint(&mut canvas, remaining);
    }
    draw_refresh_pill(&mut canvas, typography());
    if labels {
        draw_labels(&mut canvas, model);
    }

    canvas.downsample_bgra()
}

struct Canvas {
    // Premultiplied linear-enough sRGBA floats, kept deterministic and cheap.
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            pixels: vec![[0.0; 4]; HIGH_SIZE * HIGH_SIZE],
        }
    }

    fn blend(&mut self, x: usize, y: usize, rgb: [u8; 3], alpha: f32) {
        if x >= HIGH_SIZE || y >= HIGH_SIZE {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let source = [
            f32::from(rgb[0]) / 255.0 * alpha,
            f32::from(rgb[1]) / 255.0 * alpha,
            f32::from(rgb[2]) / 255.0 * alpha,
            alpha,
        ];
        let destination = &mut self.pixels[y * HIGH_SIZE + x];
        let keep = 1.0 - source[3];
        destination[0] = source[0] + destination[0] * keep;
        destination[1] = source[1] + destination[1] * keep;
        destination[2] = source[2] + destination[2] * keep;
        destination[3] = source[3] + destination[3] * keep;
    }

    fn downsample_bgra(self) -> Vec<u8> {
        let mut output = vec![0; OUTPUT_SIZE * OUTPUT_SIZE * 4];
        for y in 0..OUTPUT_SIZE {
            for x in 0..OUTPUT_SIZE {
                let mut sum = [0.0; 4];
                for sample_y in 0..SCALE {
                    for sample_x in 0..SCALE {
                        let source =
                            self.pixels[(y * SCALE + sample_y) * HIGH_SIZE + x * SCALE + sample_x];
                        for channel in 0..4 {
                            sum[channel] += source[channel];
                        }
                    }
                }
                let divisor = (SCALE * SCALE) as f32;
                let offset = (y * OUTPUT_SIZE + x) * 4;
                // UpdateLayeredWindow expects premultiplied BGRA.
                output[offset] = to_byte(sum[2] / divisor);
                output[offset + 1] = to_byte(sum[1] / divisor);
                output[offset + 2] = to_byte(sum[0] / divisor);
                output[offset + 3] = to_byte(sum[3] / divisor);
            }
        }
        output
    }
}

fn draw_shadow(canvas: &mut Canvas) {
    for_each_sample(
        |canvas, x, y, px, py| {
            let distance = distance(px, py, CENTER_X, CENTER_Y + 4.0);
            let outside = (distance - 51.5).max(0.0);
            if outside < 11.0 {
                let alpha = 0.42 * (-outside * outside / 36.0).exp();
                canvas.blend(x, y, [64, 12, 8], alpha);
            }
        },
        canvas,
    );
}

fn draw_outer_ambient_glow(canvas: &mut Canvas) {
    for_each_sample(
        |canvas, x, y, px, py| {
            let ring_distance = (distance(px, py, CENTER_X, CENTER_Y) - RING_RADIUS).abs();
            if ring_distance < 11.0 {
                let alpha = 0.11 * (-(ring_distance - 3.0).max(0.0).powi(2) / 18.0).exp();
                canvas.blend(x, y, [255, 68, 16], alpha);
            }
        },
        canvas,
    );
}

fn draw_track(canvas: &mut Canvas) {
    draw_ring(canvas, |_progress| Some(([112, 37, 43], 0.98)));
}

fn draw_available_glow(canvas: &mut Canvas, remaining: u8) {
    let fraction = f32::from(remaining.min(100)) / 100.0;
    if fraction <= 0.0 {
        return;
    }
    for_each_sample(
        |canvas, x, y, px, py| {
            let dx = px - CENTER_X;
            let dy = py - CENTER_Y;
            let progress = clockwise_progress(dx, dy);
            if !arc_contains(progress, fraction) {
                return;
            }
            let ring_distance = (dx.hypot(dy) - RING_RADIUS).abs();
            if ring_distance < 10.0 {
                let alpha = 0.24 * (-(ring_distance - 2.8).max(0.0).powi(2) / 8.0).exp();
                let color = available_color((progress / fraction).clamp(0.0, 1.0));
                canvas.blend(x, y, color, alpha);
            }
        },
        canvas,
    );
}

fn draw_available_ring(canvas: &mut Canvas, remaining: u8) {
    let fraction = f32::from(remaining.min(100)) / 100.0;
    if fraction <= 0.0 {
        return;
    }
    draw_ring(canvas, |progress| {
        arc_contains(progress, fraction)
            .then(|| (available_color((progress / fraction).clamp(0.0, 1.0)), 1.0))
    });
}

fn draw_ring(canvas: &mut Canvas, mut paint: impl FnMut(f32) -> Option<([u8; 3], f32)>) {
    for y in 0..HIGH_SIZE {
        for x in 0..HIGH_SIZE {
            let (px, py) = logical_point(x, y);
            let dx = px - CENTER_X;
            let dy = py - CENTER_Y;
            let signed = (dx.hypot(dy) - RING_RADIUS).abs();
            let coverage = (3.6 - signed).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            if let Some((color, alpha)) = paint(clockwise_progress(dx, dy)) {
                canvas.blend(x, y, color, coverage * alpha);
            }
        }
    }
}

fn draw_inner_disc(canvas: &mut Canvas) {
    for_each_sample(
        |canvas, x, y, px, py| {
            let dx = px - CENTER_X;
            let dy = py - CENTER_Y;
            let distance = dx.hypot(dy);
            let coverage = (46.3 - distance).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                return;
            }
            let vertical = ((py - (CENTER_Y - 46.3)) / 92.6).clamp(0.0, 1.0);
            let radial = (distance / 46.3).clamp(0.0, 1.0);
            let top = [60.0, 25.0, 29.0];
            let bottom = [20.0, 11.0, 17.0];
            let mut color = lerp_color(top, bottom, vertical * 0.78 + radial * 0.22);
            let warm = 1.0 - ((dx + 14.0).hypot(dy + 18.0) / 62.0).clamp(0.0, 1.0);
            color[0] = (f32::from(color[0]) + warm * 12.0).min(255.0) as u8;
            color[1] = (f32::from(color[1]) + warm * 4.0).min(255.0) as u8;
            canvas.blend(x, y, color, coverage * 0.98);
        },
        canvas,
    );
}

fn draw_inner_highlight(canvas: &mut Canvas) {
    for_each_sample(
        |canvas, x, y, px, py| {
            let normalized = ((px - (CENTER_X - 12.0)) / 34.0).powi(2)
                + ((py - (CENTER_Y - 22.0)) / 15.0).powi(2);
            if normalized < 1.0 && distance(px, py, CENTER_X, CENTER_Y) < 43.8 {
                canvas.blend(x, y, [255, 167, 116], (1.0 - normalized) * 0.11);
            }
        },
        canvas,
    );
}

fn draw_endpoint(canvas: &mut Canvas, remaining: u8) {
    let fraction = f32::from(remaining.min(100)) / 100.0;
    let angle = -TAU / 4.0 + TAU * fraction;
    let x = CENTER_X + RING_RADIUS * angle.cos();
    let y = CENTER_Y + RING_RADIUS * angle.sin();
    draw_soft_disc(canvas, x, y, 7.0, [255, 117, 18], 0.26);
    draw_soft_disc(canvas, x, y, 3.15, [255, 249, 220], 1.0);
    draw_soft_disc(canvas, x - 0.7, y - 0.8, 1.15, [255, 255, 255], 1.0);
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Typography {
    show_title: bool,
    title_top: f32,
    title_font_size: f32,
    title_height: f32,
    refresh_top: f32,
    refresh_font_size: f32,
    refresh_height: f32,
    refresh_pill_left: f32,
    refresh_pill_top: f32,
    refresh_pill_width: f32,
    refresh_pill_height: f32,
}

fn typography() -> Typography {
    Typography {
        show_title: false,
        title_top: 59.0,
        // User direction: title text becomes half of the previous 12 px size.
        title_font_size: 6.0,
        title_height: 10.0,
        // With the title removed, the refresh line moves into the disc's
        // central safe area instead of sitting on its narrowing lower edge.
        refresh_top: 60.0,
        refresh_font_size: 11.0,
        refresh_height: 20.0,
        refresh_pill_left: 20.0,
        refresh_pill_top: 60.0,
        refresh_pill_width: 76.0,
        refresh_pill_height: 20.0,
    }
}

fn draw_refresh_pill(canvas: &mut Canvas, typography: Typography) {
    draw_rounded_rect(
        canvas,
        typography.refresh_pill_left + CONTENT_PADDING,
        typography.refresh_pill_top + CONTENT_PADDING,
        typography.refresh_pill_width,
        typography.refresh_pill_height,
        typography.refresh_pill_height / 2.0,
        [113, 47, 35],
        0.82,
    );
    draw_rounded_rect(
        canvas,
        typography.refresh_pill_left + CONTENT_PADDING + 1.0,
        typography.refresh_pill_top + CONTENT_PADDING + 1.0,
        typography.refresh_pill_width - 2.0,
        typography.refresh_pill_height - 2.0,
        (typography.refresh_pill_height - 2.0) / 2.0,
        [45, 20, 21],
        0.92,
    );
}

fn draw_labels(canvas: &mut Canvas, model: &WeeklyQuotaRenderModel) {
    let typography = typography();
    let fonts = fonts();
    if let Some(display) = fonts.display.as_ref().or(fonts.ui.as_ref()) {
        draw_text(
            canvas,
            display,
            &model.percent_label,
            19.0 + CONTENT_PADDING,
            30.0,
            36.0,
            [255, 247, 240],
            1.0,
        );
    }
    if let Some(ui) = fonts.ui.as_ref().or(fonts.display.as_ref()) {
        if typography.show_title {
            draw_text(
                canvas,
                ui,
                &model.title,
                typography.title_top + CONTENT_PADDING,
                typography.title_font_size,
                typography.title_height,
                [255, 229, 211],
                0.98,
            );
        }
        draw_text(
            canvas,
            ui,
            &model.refresh_label,
            typography.refresh_top + CONTENT_PADDING,
            typography.refresh_font_size,
            typography.refresh_height,
            [255, 207, 166],
            0.98,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    canvas: &mut Canvas,
    font: &Font,
    text: &str,
    top: f32,
    size: f32,
    height: f32,
    color: [u8; 3],
    opacity: f32,
) {
    let fonts = [font.clone()];
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        y: top * SCALE as f32,
        max_width: Some(HIGH_SIZE as f32),
        max_height: Some(height * SCALE as f32),
        horizontal_align: HorizontalAlign::Center,
        vertical_align: VerticalAlign::Middle,
        ..LayoutSettings::default()
    });
    layout.append(&fonts, &TextStyle::new(text, size * SCALE as f32, 0));
    for glyph in layout.glyphs() {
        let (metrics, coverage) = fonts[glyph.font_index].rasterize_config(glyph.key);
        for row in 0..metrics.height {
            for column in 0..metrics.width {
                let x = glyph.x.round() as isize + column as isize;
                let y = glyph.y.round() as isize + row as isize;
                if x < 0 || y < 0 || x >= HIGH_SIZE as isize || y >= HIGH_SIZE as isize {
                    continue;
                }
                let alpha = f32::from(coverage[row * metrics.width + column]) / 255.0;
                canvas.blend(x as usize, y as usize, color, alpha * opacity);
            }
        }
    }
}

fn draw_soft_disc(
    canvas: &mut Canvas,
    center_x: f32,
    center_y: f32,
    radius: f32,
    color: [u8; 3],
    opacity: f32,
) {
    for_each_sample(
        |canvas, x, y, px, py| {
            let distance = distance(px, py, center_x, center_y);
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if coverage > 0.0 {
                canvas.blend(x, y, color, coverage * opacity);
            }
        },
        canvas,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_rounded_rect(
    canvas: &mut Canvas,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: [u8; 3],
    opacity: f32,
) {
    let center_x = left + width / 2.0;
    let center_y = top + height / 2.0;
    let half_x = width / 2.0 - radius;
    let half_y = height / 2.0 - radius;
    for_each_sample(
        |canvas, x, y, px, py| {
            let qx = (px - center_x).abs() - half_x;
            let qy = (py - center_y).abs() - half_y;
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            let signed = outside + inside - radius;
            let coverage = (0.5 - signed).clamp(0.0, 1.0);
            if coverage > 0.0 {
                canvas.blend(x, y, color, coverage * opacity);
            }
        },
        canvas,
    );
}

fn for_each_sample(mut draw: impl FnMut(&mut Canvas, usize, usize, f32, f32), canvas: &mut Canvas) {
    for y in 0..HIGH_SIZE {
        for x in 0..HIGH_SIZE {
            let (px, py) = logical_point(x, y);
            draw(canvas, x, y, px, py);
        }
    }
}

fn logical_point(x: usize, y: usize) -> (f32, f32) {
    (
        (x as f32 + 0.5) / SCALE as f32,
        (y as f32 + 0.5) / SCALE as f32,
    )
}

fn clockwise_progress(dx: f32, dy: f32) -> f32 {
    (dy.atan2(dx) + TAU / 4.0).rem_euclid(TAU) / TAU
}

fn arc_contains(progress: f32, fraction: f32) -> bool {
    fraction >= 1.0 || progress <= fraction + 0.000_1
}

fn available_color(progress: f32) -> [u8; 3] {
    if progress < 0.48 {
        lerp_color([255.0, 188.0, 25.0], [255.0, 101.0, 10.0], progress / 0.48)
    } else {
        lerp_color(
            [255.0, 101.0, 10.0],
            [255.0, 48.0, 28.0],
            (progress - 0.48) / 0.52,
        )
    }
}

fn lerp_color(from: [f32; 3], to: [f32; 3], amount: f32) -> [u8; 3] {
    let amount = amount.clamp(0.0, 1.0);
    [
        (from[0] + (to[0] - from[0]) * amount).round() as u8,
        (from[1] + (to[1] - from[1]) * amount).round() as u8,
        (from[2] + (to[2] - from[2]) * amount).round() as u8,
    ]
}

fn distance(x: f32, y: f32, center_x: f32, center_y: f32) -> f32 {
    (x - center_x).hypot(y - center_y)
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

struct WidgetFonts {
    display: Option<Font>,
    ui: Option<Font>,
}

fn fonts() -> &'static WidgetFonts {
    static FONTS: OnceLock<WidgetFonts> = OnceLock::new();
    FONTS.get_or_init(|| WidgetFonts {
        display: load_font(&display_font_candidates()),
        ui: load_font(&ui_font_candidates()),
    })
}

fn load_font(candidates: &[PathBuf]) -> Option<Font> {
    candidates.iter().find_map(|path| {
        fs::read(path)
            .ok()
            .and_then(|bytes| Font::from_bytes(bytes, FontSettings::default()).ok())
    })
}

fn display_font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let fonts = PathBuf::from(windows).join("Fonts");
        candidates.extend([fonts.join("segoeuib.ttf"), fonts.join("seguisb.ttf")]);
    }
    candidates.extend([
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Arial Bold.ttf"),
    ]);
    candidates
}

fn ui_font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let fonts = PathBuf::from(windows).join("Fonts");
        candidates.extend([
            fonts.join("msyhbd.ttc"),
            fonts.join("msyh.ttc"),
            fonts.join("seguisb.ttf"),
        ]);
    }
    candidates.push(PathBuf::from(
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ));
    candidates
}

#[cfg(test)]
mod tests {
    use super::{
        CENTER_X, CENTER_Y, OUTPUT_SIZE, RING_RADIUS, WeeklyQuotaRenderModel, render_weekly_quota,
        typography,
    };

    fn pixel_alpha(pixels: &[u8], x: usize, y: usize) -> u8 {
        pixels[(y * OUTPUT_SIZE + x) * 4 + 3]
    }

    fn pixel_luma(pixels: &[u8], x: usize, y: usize) -> u16 {
        let offset = (y * OUTPUT_SIZE + x) * 4;
        u16::from(pixels[offset]) + u16::from(pixels[offset + 1]) + u16::from(pixels[offset + 2])
    }

    #[test]
    fn renders_transparent_antialiased_widget_canvas() {
        let pixels = render_weekly_quota(&WeeklyQuotaRenderModel::preview(78));

        assert_eq!(pixels.len(), OUTPUT_SIZE * OUTPUT_SIZE * 4);
        for (x, y) in [
            (0, 0),
            (OUTPUT_SIZE - 1, 0),
            (0, OUTPUT_SIZE - 1),
            (OUTPUT_SIZE - 1, OUTPUT_SIZE - 1),
        ] {
            assert_eq!(pixel_alpha(&pixels, x, y), 0);
        }
        assert!(pixel_alpha(&pixels, CENTER_X as usize, CENTER_Y as usize) > 240);
    }

    #[test]
    fn remaining_arc_is_bright_and_endpoint_tracks_percentage() {
        let pixels = render_weekly_quota(&WeeklyQuotaRenderModel::preview(78));

        // 78% ends at 190.8 degrees: the exact endpoint lies left and slightly below center.
        assert!(pixel_luma(&pixels, 16, 54) > 600);
        // The following part of the ring is the warm, dim consumed track.
        assert!(pixel_luma(&pixels, 36, 28) < 430);
    }

    #[test]
    fn edge_states_do_not_invent_a_marker() {
        let empty = render_weekly_quota(&WeeklyQuotaRenderModel::empty());
        let zero = render_weekly_quota(&WeeklyQuotaRenderModel::preview(0));
        let full = render_weekly_quota(&WeeklyQuotaRenderModel::preview(100));

        let top = (CENTER_Y - RING_RADIUS) as usize;
        assert!(pixel_luma(&empty, CENTER_X as usize, top) < 500);
        assert!(pixel_luma(&zero, CENTER_X as usize, top) > 500);
        assert!(pixel_luma(&full, CENTER_X as usize, top) > 500);
    }

    #[test]
    fn typography_hides_title_and_uses_a_safe_refresh_text_size() {
        let typography = typography();

        assert_eq!(typography.title_font_size, 6.0);
        assert_eq!(typography.refresh_font_size, 11.0);
    }

    #[test]
    fn typography_centers_refresh_pill_inside_inner_disc() {
        let typography = typography();

        assert!(!typography.show_title);
        assert_eq!(typography.refresh_pill_left, 20.0);
        assert_eq!(typography.refresh_pill_top, 60.0);
        assert_eq!(typography.refresh_pill_width, 76.0);
        assert_eq!(typography.refresh_pill_height, 20.0);
    }

    #[test]
    fn canvas_reserves_a_transparent_margin_for_the_soft_shadow() {
        let pixels = render_weekly_quota(&WeeklyQuotaRenderModel::preview(78));

        assert_eq!(OUTPUT_SIZE, 132);
        assert_eq!(pixel_alpha(&pixels, 0, 68), 0);
        assert!(pixel_alpha(&pixels, 4, 68) > 0);
        assert_eq!(pixel_alpha(&pixels, OUTPUT_SIZE - 1, 68), 0);
    }
}
