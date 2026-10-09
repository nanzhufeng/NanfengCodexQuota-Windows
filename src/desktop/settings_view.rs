//! Native settings presentation. Controls retain their Windows semantics and keyboard behavior.
use super::*;
use chrono::Local;
use std::cell::Cell;
use windows_sys::Win32::{
    Graphics::{Dwm::DwmSetWindowAttribute, GdiPlus::*},
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{GetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
    },
};

const DARK: u32 = 0xff142536;
const MUTED: u32 = 0xff75818c;
const GREEN: u32 = 0xff07975b;
const ORANGE: u32 = 0xffff7d16;
const LINE: u32 = 0xffe8edef;
const WHITE: u32 = 0xffffffff;

pub(super) struct GraphicsRuntime(usize);
impl GraphicsRuntime {
    pub(super) fn start() -> Result<Self> {
        let mut token = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        anyhow::ensure!(
            unsafe { GdiplusStartup(&mut token, &input, null_mut()) } == 0,
            "无法初始化设置绘图"
        );
        std::result::Result::Ok(Self(token))
    }
}
impl Drop for GraphicsRuntime {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.0);
        }
    }
}

struct Canvas {
    graphics: *mut GpGraphics,
}
impl Canvas {
    unsafe fn new(dc: HDC) -> Self {
        let mut graphics = null_mut();
        GdipCreateFromHDC(dc, &mut graphics);
        if !graphics.is_null() {
            GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        }
        Self { graphics }
    }
    unsafe fn rounded(
        &self,
        r: (f32, f32, f32, f32),
        radius: f32,
        color: u32,
        border: Option<u32>,
    ) {
        if self.graphics.is_null() {
            return;
        }
        let (x, y, w, h) = r;
        let d = (radius * 2.0).min(w).min(h);
        let mut path = null_mut();
        GdipCreatePath(FillModeAlternate, &mut path);
        if path.is_null() {
            return;
        }
        for (ax, ay, start) in [
            (x, y, 180.0),
            (x + w - d, y, 270.0),
            (x + w - d, y + h - d, 0.0),
            (x, y + h - d, 90.0),
        ] {
            GdipAddPathArc(path, ax, ay, d, d, start, 90.0);
        }
        GdipClosePathFigure(path);
        let mut brush = null_mut();
        GdipCreateSolidFill(color, &mut brush);
        if !brush.is_null() {
            GdipFillPath(self.graphics, brush.cast(), path);
            GdipDeleteBrush(brush.cast());
        }
        if let Some(color) = border {
            let mut pen = null_mut();
            GdipCreatePen1(color, 1.0, UnitPixel, &mut pen);
            if !pen.is_null() {
                GdipDrawPath(self.graphics, pen, path);
                GdipDeletePen(pen);
            }
        }
        GdipDeletePath(path);
    }
    unsafe fn arc(&self, r: (f32, f32, f32, f32), width: f32, color: u32, start: f32, sweep: f32) {
        if self.graphics.is_null() || sweep <= 0.0 {
            return;
        }
        let mut pen = null_mut();
        GdipCreatePen1(color, width, UnitPixel, &mut pen);
        if pen.is_null() {
            return;
        }
        GdipSetPenStartCap(pen, LineCapRound);
        GdipSetPenEndCap(pen, LineCapRound);
        let (x, y, w, h) = r;
        GdipDrawArc(self.graphics, pen, x, y, w, h, start, sweep);
        GdipDeletePen(pen);
    }
}
impl Drop for Canvas {
    fn drop(&mut self) {
        if !self.graphics.is_null() {
            unsafe {
                GdipDeleteGraphics(self.graphics);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Label {
        size: i32,
        weight: i32,
        center: bool,
    },
    Icon,
    Nav {
        selected: bool,
    },
    Badge,
    Action,
    Secondary,
    Toggle,
    Choice,
    Link,
}
struct Control {
    kind: Kind,
    font: HFONT,
    color: Cell<u32>,
    hover: Cell<bool>,
    tracking: Cell<bool>,
}
impl Drop for Control {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.font);
        }
    }
}
fn colorref(argb: u32) -> u32 {
    rgb((argb >> 16) as u8, (argb >> 8) as u8, argb as u8)
}

pub(super) unsafe fn configure_chrome(hwnd: HWND) {
    let caption = rgb(248, 250, 250);
    let text = rgb(20, 37, 54);
    let corner = 2u32;
    DwmSetWindowAttribute(hwnd, 35, (&caption as *const u32).cast(), 4);
    DwmSetWindowAttribute(hwnd, 36, (&text as *const u32).cast(), 4);
    DwmSetWindowAttribute(hwnd, 33, (&corner as *const u32).cast(), 4);
}
unsafe fn dimensions(hwnd: HWND) -> (i32, i32, u32) {
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    let dpi = GetDpiForWindow(hwnd).max(96);
    (
        mul_div(rect.right, 96, dpi as i32),
        mul_div(rect.bottom, 96, dpi as i32),
        dpi,
    )
}

#[derive(Clone, Copy)]
struct Layout {
    width: i32,
    height: i32,
    divider: i32,
    right: i32,
    left_width: i32,
    ring: i32,
    ring_x: i32,
    ring_y: i32,
}
impl Layout {
    fn new(width: i32, height: i32) -> Self {
        let divider = (width as f32 * 0.43) as i32;
        let left_width = divider - 68;
        let ring = ((left_width as f32 * 0.73) as i32)
            .min(height - 386)
            .clamp(190, 280);
        Self {
            width,
            height,
            divider,
            right: divider + 40,
            left_width,
            ring,
            ring_x: 34 + (left_width - ring) / 2,
            ring_y: 190,
        }
    }
    fn rect(self, id: usize) -> (i32, i32, i32, i32) {
        let right_width = self.width - self.right - 36;
        let data = self.ring_y + self.ring + 20;
        let center = self.ring_y + self.ring / 2;
        match id {
            T_GENERAL => (self.width - 234, 22, 90, 54),
            T_ABOUT => (self.width - 124, 22, 90, 54),
            GITHUB_LINK => {
                let width = 660.min(self.width - 96);
                ((self.width - width) / 2 + 24, 414, width - 48, 28)
            }
            502 => (34, 108, self.left_width, 36),
            503 => (34, 151, 150, 30),
            504 => (self.ring_x + self.ring / 2 - 80, center - 42, 160, 70),
            505 => (self.ring_x + self.ring / 2 - 50, center + 29, 100, 26),
            506 => (68, data, 114, 28),
            507 => (180, data, self.divider - 206, 28),
            508 => (68, data + 34, 114, 28),
            509 => (180, data + 34, self.divider - 206, 28),
            510 => (68, data + 68, 114, 28),
            511 => (180, data + 68, self.divider - 206, 28),
            512 => (self.right, 108, right_width, 36),
            513 => (self.right, 151, right_width, 30),
            514 => (self.right + 52, 226, right_width - 258, 34),
            515 => (self.right + 52, 332, right_width - 156, 34),
            516 => (self.right + 52, 438, right_width - 156, 34),
            517 => (self.right, self.height - 52, right_width, 28),
            INTERVAL => (self.width - 216, 220, 180, 250),
            TOP_CHECK => (self.width - 110, 332, 72, 34),
            STARTUP => (self.width - 110, 438, 72, 34),
            REFRESH => (34, self.height - 78, self.left_width, 50),
            POSITION => (self.right, self.height - 155, 282, 50),
            601 => (34, data, 25, 28),
            602 => (34, data + 34, 25, 28),
            603 => (34, data + 68, 25, 28),
            604 => (self.right, 226, 28, 34),
            605 => (self.right, 332, 28, 34),
            606 => (self.right, 438, 28, 34),
            _ => (0, 0, 1, 1),
        }
    }
}
pub(super) unsafe fn rebuild(app: &mut App) {
    if app.settings.is_null() {
        return;
    }
    for hwnd in app.controls.drain(..) {
        DestroyWindow(hwnd);
    }
    let (_, _, dpi) = dimensions(app.settings);
    DeleteObject(app.font);
    DeleteObject(app.title_font);
    app.font = make_font(16, 400, dpi);
    app.title_font = make_font(24, 600, dpi);
    add(
        app,
        T_GENERAL,
        "常规",
        Kind::Nav {
            selected: app.page == T_GENERAL,
        },
        DARK,
    );
    add(
        app,
        T_ABOUT,
        "关于",
        Kind::Nav {
            selected: app.page == T_ABOUT,
        },
        DARK,
    );
    if app.page == T_GENERAL {
        for (id, text, size, weight, color, center) in [
            (502, "周剩余额度", 24, 600, DARK, false),
            (504, "--%", 54, 700, DARK, true),
            (505, "剩余", 16, 400, MUTED, true),
            (506, "下次重置", 16, 400, DARK, false),
            (507, "--", 16, 400, DARK, false),
            (508, "最近成功", 16, 400, DARK, false),
            (509, "--", 16, 400, DARK, false),
            (510, "五小时", 16, 400, MUTED, false),
            (511, "接口未提供", 16, 400, MUTED, false),
            (512, "悬浮窗偏好", 24, 600, DARK, false),
            (513, "设置即时生效。", 16, 400, MUTED, false),
            (514, "刷新间隔", 18, 400, DARK, false),
            (515, "保持悬浮窗置顶", 18, 400, DARK, false),
            (516, "登录 Windows 时启动", 18, 400, DARK, false),
            (517, "只读监控，不调用模型。", 14, 400, MUTED, false),
        ] {
            add(
                app,
                id,
                text,
                Kind::Label {
                    size,
                    weight,
                    center,
                },
                color,
            );
        }
        add(app, 503, "等待查询", Kind::Badge, MUTED);
        for (id, glyph) in [
            (601, "\u{e787}"),
            (602, "\u{e823}"),
            (603, "\u{e711}"),
            (604, "\u{e823}"),
            (605, "\u{e718}"),
            (606, "\u{e7e8}"),
        ] {
            add(
                app,
                id,
                glyph,
                Kind::Icon,
                if id == 603 { MUTED } else { DARK },
            );
        }
        let combo = add(app, INTERVAL, "刷新间隔", Kind::Choice, DARK);
        for seconds in [30, 60, 120, 300, 600] {
            SendMessageW(
                combo,
                CB_ADDSTRING,
                0,
                wide(&format!("{seconds} 秒")).as_ptr() as isize,
            );
        }
        let index = [30, 60, 120, 300, 600]
            .iter()
            .position(|s| *s == app.config.interval_secs)
            .unwrap_or(1);
        SendMessageW(combo, CB_SETCURSEL, index, 0);
        SendMessageW(
            combo,
            CB_SETITEMHEIGHT,
            0,
            mul_div(36, dpi as i32, 96) as isize,
        );
        SendMessageW(
            combo,
            CB_SETITEMHEIGHT,
            usize::MAX,
            mul_div(38, dpi as i32, 96) as isize,
        );
        let top = add(app, TOP_CHECK, "保持悬浮窗置顶", Kind::Toggle, GREEN);
        SendMessageW(top, BM_SETCHECK, usize::from(app.config.always_on_top), 0);
        let startup = add(app, STARTUP, "登录 Windows 时启动", Kind::Toggle, GREEN);
        SendMessageW(
            startup,
            BM_SETCHECK,
            usize::from(app.config.start_at_login),
            0,
        );
        add(app, REFRESH, "立即刷新", Kind::Action, WHITE);
        add(app, POSITION, "找回悬浮窗", Kind::Secondary, DARK);
    } else {
        add(
            app,
            GITHUB_LINK,
            "GitHub · nanzhufeng/NanfengCodexQuota-Windows ↗",
            Kind::Link,
            0xff2768a6,
        );
    }
    layout(app);
    update(app);
}
unsafe fn add(app: &mut App, id: usize, text: &str, kind: Kind, color: u32) -> HWND {
    let (_, _, dpi) = dimensions(app.settings);
    let (class, style) = match kind {
        Kind::Choice => (
            "COMBOBOX",
            WS_TABSTOP
                | CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32
                | WS_VSCROLL,
        ),
        Kind::Toggle => ("BUTTON", WS_TABSTOP | BS_AUTOCHECKBOX as u32),
        Kind::Nav { .. } | Kind::Action | Kind::Secondary | Kind::Link => {
            ("BUTTON", WS_TABSTOP | BS_PUSHBUTTON as u32)
        }
        _ => ("STATIC", 0),
    };
    let hwnd = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        0,
        0,
        1,
        1,
        app.settings,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(hwnd, WM_SETFONT, app.font as usize, 0);
    let (size, weight, face) = match kind {
        Kind::Label { size, weight, .. } => (size, weight, "Microsoft YaHei UI"),
        Kind::Icon => (24, 400, "Segoe MDL2 Assets"),
        Kind::Nav { .. } => (18, 600, "Microsoft YaHei UI"),
        _ => (16, 400, "Microsoft YaHei UI"),
    };
    let font = CreateFontW(
        -mul_div(size, dpi as i32, 96),
        0,
        0,
        0,
        weight,
        0,
        u32::from(matches!(kind, Kind::Link)),
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide(face).as_ptr(),
    );
    let data = Box::into_raw(Box::new(Control {
        kind,
        font,
        color: Cell::new(color),
        hover: Cell::new(false),
        tracking: Cell::new(false),
    }));
    if SetWindowSubclass(hwnd, Some(control_proc), 1, data as usize) == 0 {
        drop(Box::from_raw(data));
    }
    app.controls.push(hwnd);
    hwnd
}
pub(super) unsafe fn layout(app: &mut App) {
    if app.settings.is_null() {
        return;
    }
    let (w, h, dpi) = dimensions(app.settings);
    let view = Layout::new(w, h);
    for &hwnd in &app.controls {
        let id = GetDlgCtrlID(hwnd) as usize;
        let (x, y, width, height) = view.rect(id);
        SetWindowPos(
            hwnd,
            null_mut(),
            mul_div(x, dpi as i32, 96),
            mul_div(y, dpi as i32, 96),
            mul_div(width, dpi as i32, 96),
            mul_div(height, dpi as i32, 96),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    InvalidateRect(app.settings, null(), 1);
}
unsafe fn set_label(hwnd: HWND, text: &str, color: u32) {
    if hwnd.is_null() {
        return;
    }
    let mut data = 0;
    if GetWindowSubclass(hwnd, Some(control_proc), 1, &mut data) != 0 {
        (*(data as *const Control)).color.set(color);
    }
    SetWindowTextW(hwnd, wide(text).as_ptr());
    InvalidateRect(hwnd, null(), 0);
}
pub(super) unsafe fn update(app: &mut App) {
    if app.settings.is_null() {
        return;
    }
    if app.page == T_GENERAL {
        let now = Utc::now();
        let remaining = app
            .monitor
            .limits
            .as_ref()
            .and_then(|l| l.secondary.remaining_percent());
        set_label(
            GetDlgItem(app.settings, 504),
            &remaining
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "--%".into()),
            DARK,
        );
        let (status, color) = if let Some(f) = app.monitor.failure {
            (f.label(), 0xffbc6808)
        } else if app.monitor.refreshing {
            ("刷新中", MUTED)
        } else if app.monitor.stale(now, app.config.interval_secs) {
            ("数据过期", 0xffbc6808)
        } else if app.monitor.limits.is_some() {
            ("已连接", GREEN)
        } else {
            ("等待查询", MUTED)
        };
        set_label(GetDlgItem(app.settings, 503), status, color);
        let reset = app
            .monitor
            .limits
            .as_ref()
            .and_then(|l| l.secondary.resets_at)
            .map(|t| {
                t.with_timezone(&Local)
                    .format("%m月%d日  %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "接口未提供".into());
        let sampled = app
            .monitor
            .limits
            .as_ref()
            .map(|l| {
                l.sampled_at
                    .with_timezone(&Local)
                    .format("%m月%d日  %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "尚无成功记录".into());
        let short = app
            .monitor
            .limits
            .as_ref()
            .and_then(|l| l.primary.remaining_percent())
            .map(|p| format!("剩余 {p}%"))
            .unwrap_or_else(|| "接口未提供".into());
        for (id, value) in [(507, reset), (509, sampled), (511, short)] {
            set_label(
                GetDlgItem(app.settings, id),
                &value,
                if id == 511 { MUTED } else { DARK },
            );
        }
        let button = GetDlgItem(app.settings, REFRESH as i32);
        SetWindowTextW(
            button,
            wide(if app.monitor.refreshing {
                "刷新中"
            } else {
                "立即刷新"
            })
            .as_ptr(),
        );
        EnableWindow(button, i32::from(!app.monitor.refreshing));
    }
    InvalidateRect(app.settings, null(), 0);
}

unsafe extern "system" fn control_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    reference: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(control_proc), 1);
        let result = DefSubclassProc(hwnd, msg, w, l);
        drop(Box::from_raw(reference as *mut Control));
        return result;
    }
    let data = &*(reference as *const Control);
    match msg {
        WM_SETCURSOR if matches!(data.kind, Kind::Link) => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            draw_control(hwnd, dc, data);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            if !data.hover.replace(true) {
                InvalidateRect(hwnd, null(), 0);
            }
            if !data.tracking.replace(true) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
            }
            DefSubclassProc(hwnd, msg, w, l)
        }
        WM_MOUSELEAVE => {
            data.hover.set(false);
            data.tracking.set(false);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_LBUTTONDOWN | WM_LBUTTONUP | BM_SETCHECK | BM_CLICK
        | CB_SETCURSEL | WM_ENABLE => {
            let result = DefSubclassProc(hwnd, msg, w, l);
            InvalidateRect(hwnd, null(), 0);
            result
        }
        _ => DefSubclassProc(hwnd, msg, w, l),
    }
}
unsafe fn text(dc: HDC, font: HFONT, value: &str, rect: RECT, color: u32, flags: u32) {
    let old = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, colorref(color));
    let mut rect = rect;
    DrawTextW(dc, wide(value).as_ptr(), -1, &mut rect, flags);
    SelectObject(dc, old);
}
pub(super) unsafe fn draw_choice(item: &DRAWITEMSTRUCT) {
    let selected = item.itemState & ODS_SELECTED != 0;
    let brush = CreateSolidBrush(colorref(if selected { 0xffe1f6eb } else { WHITE }));
    FillRect(item.hDC, &item.rcItem, brush);
    DeleteObject(brush);
    if let Some(seconds) = [30, 60, 120, 300, 600].get(item.itemID as usize) {
        let mut rect = item.rcItem;
        rect.left += mul_div(16, GetDpiForWindow(item.hwndItem).max(96) as i32, 96);
        text(
            item.hDC,
            SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT,
            &format!("{seconds} 秒"),
            rect,
            if selected { GREEN } else { DARK },
            DT_VCENTER | DT_SINGLELINE,
        );
    }
}
pub(super) unsafe fn draw_menu(app: &App, item: &DRAWITEMSTRUCT) {
    let dpi = GetDpiForWindow(app.widget).max(96);
    let s = |n| mul_div(n, dpi as i32, 96);
    let r = item.rcItem;
    FillRect(item.hDC, &r, GetStockObject(WHITE_BRUSH) as HBRUSH);
    let selected = item.itemState & ODS_SELECTED != 0;
    let disabled = item.itemState & windows_sys::Win32::UI::Controls::ODS_DISABLED != 0;
    let checked = item.itemState & windows_sys::Win32::UI::Controls::ODS_CHECKED != 0;
    if selected && !disabled {
        let canvas = Canvas::new(item.hDC);
        canvas.rounded(
            (
                (r.left + s(6)) as f32,
                (r.top + s(3)) as f32,
                (r.right - r.left - s(12)) as f32,
                (r.bottom - r.top - s(6)) as f32,
            ),
            s(8) as f32,
            0xffeef7f2,
            None,
        );
    }
    let (label, glyph) = match item.itemID as usize {
        REFRESH => ("立即刷新", "\u{e72c}"),
        OPEN_SETTINGS => ("打开主界面", "\u{e80f}"),
        TOP => ("保持置顶", "\u{e718}"),
        HIDE if app.visible => ("隐藏悬浮窗", "\u{e921}"),
        HIDE => ("显示悬浮窗", "\u{e944}"),
        EXIT => ("退出", "\u{e8bb}"),
        _ => return,
    };
    let color = if disabled {
        MUTED
    } else if checked {
        GREEN
    } else {
        0xff405362
    };
    let font = CreateFontW(
        -s(18),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe MDL2 Assets").as_ptr(),
    );
    text(
        item.hDC,
        font,
        glyph,
        RECT {
            left: r.left + s(16),
            right: r.left + s(38),
            ..r
        },
        color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    DeleteObject(font);
    text(
        item.hDC,
        app.font,
        label,
        RECT {
            left: r.left + s(52),
            right: r.right - s(30),
            ..r
        },
        color,
        DT_VCENTER | DT_SINGLELINE,
    );
    if checked {
        text(
            item.hDC,
            app.font,
            "✓",
            RECT {
                left: r.right - s(28),
                ..r
            },
            GREEN,
            DT_VCENTER | DT_SINGLELINE,
        );
    }
}
unsafe fn draw_control(hwnd: HWND, dc: HDC, data: &Control) {
    let mut rect: RECT = zeroed();
    GetClientRect(hwnd, &mut rect);
    FillRect(dc, &rect, GetStockObject(WHITE_BRUSH) as HBRUSH);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let s = |n| mul_div(n, dpi as i32, 96);
    let width = rect.right;
    let height = rect.bottom;
    let canvas = Canvas::new(dc);
    let enabled = windows_sys::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(hwnd) != 0;
    let pressed = SendMessageW(hwnd, BM_GETSTATE, 0, 0) & 4 != 0;
    let mut buffer = [0u16; 256];
    let count = GetWindowTextW(hwnd, buffer.as_mut_ptr(), 256);
    let label = String::from_utf16_lossy(&buffer[..count.max(0) as usize]);
    let centered = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
    match data.kind {
        Kind::Link => text(
            dc,
            data.font,
            &label,
            rect,
            if data.hover.get() {
                GREEN
            } else {
                data.color.get()
            },
            DT_VCENTER | DT_SINGLELINE,
        ),
        Kind::Label { center, .. } => text(
            dc,
            data.font,
            &label,
            rect,
            data.color.get(),
            DT_VCENTER | DT_SINGLELINE | if center { DT_CENTER } else { DT_LEFT },
        ),
        Kind::Icon => text(dc, data.font, &label, rect, data.color.get(), centered),
        Kind::Badge => {
            let color = data.color.get();
            let bg = if color == GREEN {
                0xffe1f6eb
            } else if color == MUTED {
                0xffeef2f4
            } else {
                0xfffff2de
            };
            canvas.rounded(
                (0.0, 0.0, width as f32 - 1.0, height as f32 - 1.0),
                s(10) as f32,
                bg,
                None,
            );
            canvas.rounded(
                (
                    s(12) as f32,
                    (height - s(10)) as f32 / 2.0,
                    s(10) as f32,
                    s(10) as f32,
                ),
                s(5) as f32,
                color,
                None,
            );
            text(
                dc,
                data.font,
                &label,
                RECT {
                    left: s(30),
                    ..rect
                },
                color,
                DT_VCENTER | DT_SINGLELINE,
            );
        }
        Kind::Nav { selected } => {
            if data.hover.get() {
                canvas.rounded(
                    (1.0, 1.0, (width - 2) as f32, (height - 2) as f32),
                    s(8) as f32,
                    0xfff6faf8,
                    None,
                );
            }
            text(
                dc,
                data.font,
                &label,
                rect,
                if selected { GREEN } else { DARK },
                centered,
            );
            if selected {
                canvas.rounded(
                    (
                        s(8) as f32,
                        (height - s(3)) as f32,
                        (width - s(16)) as f32,
                        s(3) as f32,
                    ),
                    s(1) as f32,
                    GREEN,
                    None,
                );
            }
        }
        Kind::Toggle => {
            let checked = SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
            let fill = if !enabled {
                0xffdce2e5
            } else if checked {
                if pressed {
                    0xff087648
                } else if data.hover.get() {
                    0xff0baa68
                } else {
                    GREEN
                }
            } else {
                0xffb9c2ca
            };
            canvas.rounded(
                (1.0, 1.0, (width - 2) as f32, (height - 2) as f32),
                (height - 2) as f32 / 2.0,
                fill,
                None,
            );
            let knob = height - s(8);
            let x = if checked { width - knob - s(4) } else { s(4) };
            canvas.rounded(
                (x as f32, s(4) as f32, knob as f32, knob as f32),
                knob as f32 / 2.0,
                WHITE,
                None,
            );
        }
        Kind::Choice => {
            canvas.rounded(
                (1.0, 1.0, (width - 2) as f32, (height - 2) as f32),
                s(9) as f32,
                WHITE,
                Some(if GetFocus() == hwnd {
                    GREEN
                } else {
                    0xffc8d0d6
                }),
            );
            let index = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
            let value = if (0..5).contains(&index) {
                format!("{} 秒", [30, 60, 120, 300, 600][index as usize])
            } else {
                "60 秒".into()
            };
            text(
                dc,
                data.font,
                &value,
                RECT {
                    left: s(16),
                    right: width - s(35),
                    ..rect
                },
                DARK,
                DT_VCENTER | DT_SINGLELINE,
            );
            let arrow = CreateFontW(
                -s(14),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide("Segoe MDL2 Assets").as_ptr(),
            );
            text(
                dc,
                arrow,
                "\u{e70d}",
                RECT {
                    left: width - s(32),
                    right: width - s(10),
                    ..rect
                },
                DARK,
                centered,
            );
            DeleteObject(arrow);
        }
        Kind::Action | Kind::Secondary => {
            let primary = matches!(data.kind, Kind::Action);
            let color = if primary {
                if !enabled {
                    0xffffc79a
                } else if pressed {
                    0xffe96c09
                } else if data.hover.get() {
                    0xffff8c2d
                } else {
                    ORANGE
                }
            } else if pressed {
                0xffeaf0f2
            } else if data.hover.get() {
                0xfff4f8f8
            } else {
                WHITE
            };
            canvas.rounded(
                (1.0, 1.0, (width - 2) as f32, (height - 2) as f32),
                s(10) as f32,
                color,
                if primary { None } else { Some(0xffb8c3cc) },
            );
            let mut measure = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            let old = SelectObject(dc, data.font);
            DrawTextW(
                dc,
                wide(&label).as_ptr(),
                -1,
                &mut measure,
                DT_CALCRECT | DT_SINGLELINE,
            );
            SelectObject(dc, old);
            let group_width = measure.right + s(34);
            let left = (width - group_width) / 2;
            let icon = CreateFontW(
                -s(20),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide("Segoe MDL2 Assets").as_ptr(),
            );
            let ink = if primary { WHITE } else { DARK };
            text(
                dc,
                icon,
                if primary { "\u{e72c}" } else { "\u{e944}" },
                RECT {
                    left,
                    right: left + s(23),
                    ..rect
                },
                ink,
                centered,
            );
            DeleteObject(icon);
            text(
                dc,
                data.font,
                &label,
                RECT {
                    left: left + s(34),
                    ..rect
                },
                ink,
                DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
    if GetFocus() == hwnd
        && !matches!(
            data.kind,
            Kind::Label { .. } | Kind::Icon | Kind::Badge | Kind::Choice
        )
    {
        canvas.rounded(
            (2.0, 2.0, (width - 4) as f32, (height - 4) as f32),
            match data.kind {
                Kind::Toggle => (height - 4) as f32 / 2.0,
                Kind::Nav { .. } => s(6) as f32,
                _ => s(8) as f32,
            },
            0x00000000,
            Some(0xff56b990),
        );
    }
}

pub(super) unsafe fn paint(app: &App, dc: HDC) {
    let (w, h, dpi) = dimensions(app.settings);
    let layout = Layout::new(w, h);
    let s = |n| mul_div(n, dpi as i32, 96);
    let old_pen = SelectObject(dc, CreatePen(PS_SOLID, 1, colorref(LINE)));
    MoveToEx(dc, 0, s(86), null_mut());
    LineTo(dc, s(w), s(86));
    let pen = SelectObject(dc, old_pen);
    DeleteObject(pen);
    if app.page == T_ABOUT {
        paint_about(app, dc, w, h, dpi);
        return;
    }
    let pen = CreatePen(PS_SOLID, 1, colorref(LINE));
    let old = SelectObject(dc, pen);
    MoveToEx(dc, s(layout.divider), s(108), null_mut());
    LineTo(dc, s(layout.divider), s(h - 24));
    for y in [294, 400] {
        MoveToEx(dc, s(layout.right), s(y), null_mut());
        LineTo(dc, s(w - 36), s(y));
    }
    MoveToEx(dc, s(layout.right), s(h - 70), null_mut());
    LineTo(dc, s(w - 36), s(h - 70));
    SelectObject(dc, old);
    DeleteObject(pen);
    let canvas = Canvas::new(dc);
    let inset = 11;
    let ring = (
        s(layout.ring_x + inset) as f32,
        s(layout.ring_y + inset) as f32,
        s(layout.ring - inset * 2) as f32,
        s(layout.ring - inset * 2) as f32,
    );
    let thickness = s(21) as f32;
    canvas.arc(ring, thickness, 0xffedf1f2, -90.0, 360.0);
    if let Some(p) = app
        .monitor
        .limits
        .as_ref()
        .and_then(|l| l.secondary.remaining_percent())
    {
        let sweep = f32::from(p) * 3.6;
        canvas.arc(ring, thickness, 0xff12bb77, -90.0, sweep);
        if p > 0 {
            let tail = sweep.min(11.0);
            canvas.arc(ring, thickness, 0xffff9b1b, -90.0 + sweep - tail, tail);
        }
    }
}
unsafe fn paint_about(app: &App, dc: HDC, w: i32, h: i32, dpi: u32) {
    let s = |n| mul_div(n, dpi as i32, 96);
    let width = 660.min(w - 96);
    let left = (w - width) / 2;
    let mut title = RECT {
        left: s(left),
        top: s(109),
        right: s(left + width),
        bottom: s(147),
    };
    text(
        dc,
        app.title_font,
        "关于",
        title,
        DARK,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    let canvas = Canvas::new(dc);
    canvas.rounded(
        (
            s(left) as f32,
            s(166) as f32,
            s(width) as f32,
            s((h - 192).min(458)) as f32,
        ),
        s(16) as f32,
        WHITE,
        Some(LINE),
    );
    let commit = env!("BUILD_COMMIT");
    let commit = &commit[..commit.len().min(12)];
    let dirty = if env!("BUILD_DIRTY") == "true" {
        "（本地改动）"
    } else {
        ""
    };
    let groups = [
        (
            "产品信息",
            format!("{TITLE}\nWindows 只读额度悬浮窗，复用本机 Codex 登录。"),
        ),
        (
            "版本信息",
            format!(
                "Desktop 版 {} · 开发日期 2026-10-09\nGit Commit：{commit}{dirty}",
                env!("CARGO_PKG_VERSION")
            ),
        ),
        (
            "开发者信息",
            "开发者：席瑞\n联系邮箱：nanzhufeng.studio@gmail.com\n版权所有 © 2026 席瑞".into(),
        ),
    ];
    let heading = make_font(16, 600, dpi);
    let font = make_font(16, 400, dpi);
    for (index, (name, body)) in groups.iter().enumerate() {
        let y = 188 + index as i32 * 139;
        title = RECT {
            left: s(left + 24),
            top: s(y),
            right: s(left + width - 24),
            bottom: s(y + 28),
        };
        text(dc, heading, name, title, DARK, DT_LEFT | DT_SINGLELINE);
        text(
            dc,
            font,
            body,
            RECT {
                top: s(y + 36),
                bottom: s(y + 118),
                ..title
            },
            DARK,
            DT_LEFT | DT_WORDBREAK,
        );
        if index < 2 {
            let pen = CreatePen(PS_SOLID, 1, colorref(LINE));
            let old = SelectObject(dc, pen);
            MoveToEx(dc, s(left + 24), s(y + 126), null_mut());
            LineTo(dc, s(left + width - 24), s(y + 126));
            SelectObject(dc, old);
            DeleteObject(pen);
        }
    }
    DeleteObject(heading);
    DeleteObject(font);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supported_sizes_keep_controls_inside_window() {
        for (w, h) in [(900, 640), (1064, 711), (1400, 900)] {
            let layout = Layout::new(w, h);
            for id in [T_GENERAL, T_ABOUT, REFRESH, POSITION, TOP_CHECK, STARTUP] {
                let (x, y, cw, ch) = layout.rect(id);
                assert!(
                    x >= 0 && y >= 0 && x + cw <= w && y + ch <= h,
                    "control {id} clipped at {w}x{h}"
                );
            }
        }
    }
}
