#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    config::Config,
    monitor::{Monitor, clamp_position},
    weekly_quota_render::{OUTPUT_SIZE, WeeklyQuotaRenderModel, render_weekly_quota_base},
    worker::{Command, Event, Worker},
};
use anyhow::Result;
use chrono::Utc;
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    sync::mpsc::Receiver,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Registry::*, Threading::CreateMutexW},
    UI::{
        Controls::BST_CHECKED,
        HiDpi::*,
        Input::KeyboardAndMouse::{EnableWindow, ReleaseCapture},
        Shell::*,
        WindowsAndMessaging::*,
    },
};

const TITLE: &str = "南枫 Codex 额度";
const CLASS: &str = "NanfengCodexQuota.Widget.v1";
const SETTINGS: &str = "NanfengCodexQuota.Settings.v1";
const EVENT: u32 = WM_APP + 10;
const TRAY: u32 = WM_APP + 11;
const RESTORE: u32 = WM_APP + 12;
const REFRESH: usize = 100;
const OPEN_SETTINGS: usize = 101;
const TOP: usize = 102;
const EXIT: usize = 103;
const HIDE: usize = 104;
const INTERVAL: usize = 201;
const STARTUP: usize = 202;
const TOP_CHECK: usize = 203;
const POSITION: usize = 204;
const T_GENERAL: usize = 301;
const T_REVIEW: usize = 302;
const T_ABOUT: usize = 303;

struct App {
    config: Config,
    path: PathBuf,
    monitor: Monitor,
    worker: Option<Worker>,
    events: Option<Receiver<Event>>,
    widget: HWND,
    settings: HWND,
    size: i32,
    dpi: u32,
    visible: bool,
    page: usize,
    controls: Vec<HWND>,
    font: HFONT,
    title_font: HFONT,
    background: HBRUSH,
    tray: NOTIFYICONDATAW,
    taskbar_created: u32,
    closing: bool,
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
pub fn show_error(text: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(text).as_ptr(),
            wide(TITLE).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
fn fail(text: &str) -> anyhow::Error {
    anyhow::Error::from(std::io::Error::last_os_error()).context(text.to_owned())
}

pub fn run() -> Result<()> {
    unsafe {
        let mutex = CreateMutexW(
            null(),
            0,
            wide("Local\\NanfengCodexQuota.Standalone.v1").as_ptr(),
        );
        anyhow::ensure!(!mutex.is_null(), "无法创建单实例标识");
        struct Guard(HANDLE);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let existing = GetLastError() == ERROR_ALREADY_EXISTS;
        let _guard = Guard(mutex);
        if existing {
            let hwnd = FindWindowW(wide(CLASS).as_ptr(), null());
            if !hwnd.is_null() {
                PostMessageW(hwnd, RESTORE, 0, 0);
            }
            return Ok(());
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let path = Config::path()?;
        let config = Config::load(&path)?;
        let font = make_font(14, 400, 96);
        let title_font = make_font(22, 600, 96);
        let mut app = Box::new(App {
            config,
            path,
            monitor: Monitor::default(),
            worker: None,
            events: None,
            widget: null_mut(),
            settings: null_mut(),
            size: OUTPUT_SIZE as i32,
            dpi: 96,
            visible: true,
            page: T_GENERAL,
            controls: vec![],
            font,
            title_font,
            background: CreateSolidBrush(rgb(246, 248, 250)),
            tray: zeroed(),
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            closing: false,
        });
        let ptr = (&mut *app) as *mut App;
        let instance = GetModuleHandleW(null());
        for (name, proc) in [
            (
                CLASS,
                Some(
                    widget_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                ),
            ),
            (
                SETTINGS,
                Some(
                    settings_proc
                        as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                ),
            ),
        ] {
            let class_name = wide(name);
            let wc = WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: instance,
                lpszClassName: class_name.as_ptr(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hIcon: LoadIconW(instance, std::ptr::without_provenance(1)),
                ..zeroed()
            };
            anyhow::ensure!(RegisterClassW(&wc) != 0, "无法注册窗口类");
        }
        app.widget = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            wide(CLASS).as_ptr(),
            wide(TITLE).as_ptr(),
            WS_POPUP,
            app.config.x,
            app.config.y,
            app.size,
            app.size,
            null_mut(),
            null_mut(),
            instance,
            ptr.cast(),
        );
        anyhow::ensure!(!app.widget.is_null(), "无法创建悬浮窗");
        app.dpi = GetDpiForWindow(app.widget).max(96);
        app.size = mul_div(OUTPUT_SIZE as i32, app.dpi as i32, 96);
        restore_position(&mut app);
        present(&app)?;
        show_widget(&mut app);
        install_tray(&mut app);
        // Reconcile the actual registry with the setting; never create startup registration just by launching.
        app.config.start_at_login = startup_matches_current_exe();
        SetTimer(app.widget, 1, 30000, None);
        let hwnd = app.widget as usize;
        let (worker, events) = Worker::start(app.config.clone(), move || {
            PostMessageW(hwnd as HWND, EVENT, 0, 0);
        });
        app.worker = Some(worker);
        app.events = Some(events);
        let mut msg: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                return Err(anyhow::anyhow!("窗口消息循环失败"));
            }
            if app.settings.is_null() || IsDialogMessageW(app.settings, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        app.worker.take();
        Shell_NotifyIconW(NIM_DELETE, &app.tray);
        DeleteObject(app.font);
        DeleteObject(app.title_font);
        DeleteObject(app.background);
        Ok(())
    }
}
fn mul_div(value: i32, n: i32, d: i32) -> i32 {
    ((i64::from(value) * i64::from(n) + i64::from(d) / 2) / i64::from(d)) as i32
}
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}
unsafe fn make_font(px: i32, weight: i32, dpi: u32) -> HFONT {
    CreateFontW(
        -mul_div(px, dpi as i32, 96),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Microsoft YaHei UI").as_ptr(),
    )
}
unsafe fn app_from(hwnd: HWND) -> Option<&'static mut App> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut()
}
unsafe fn attach(hwnd: HWND, msg: u32, lparam: LPARAM) {
    if msg == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
}
unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    attach(hwnd, msg, l);
    let Some(app) = app_from(hwnd) else {
        return DefWindowProcW(hwnd, msg, w, l);
    };
    if msg == app.taskbar_created && app.taskbar_created != 0 {
        install_tray(app);
        return 0;
    }
    match msg {
        EVENT => {
            drain_events(app);
            0
        }
        WM_TIMER => {
            paint_or_report(app);
            write_diagnostics(app);
            0
        }
        WM_LBUTTONDOWN => {
            ReleaseCapture();
            SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
            0
        }
        WM_EXITSIZEMOVE => {
            save_position(app);
            0
        }
        WM_CONTEXTMENU => {
            menu(app);
            0
        }
        WM_COMMAND => {
            command(app, w & 0xffff);
            0
        }
        TRAY => {
            match l as u32 {
                WM_RBUTTONUP | WM_CONTEXTMENU => menu(app),
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => show_widget(app),
                _ => {}
            }
            0
        }
        RESTORE => {
            restore_position(app);
            show_widget(app);
            0
        }
        WM_DISPLAYCHANGE => {
            restore_position(app);
            save_position(app);
            paint_or_report(app);
            0
        }
        WM_DPICHANGED => {
            app.dpi = (w & 0xffff) as u32;
            app.size = mul_div(OUTPUT_SIZE as i32, app.dpi as i32, 96);
            let r = &*(l as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                app.size,
                app.size,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            restore_position(app);
            paint_or_report(app);
            0
        }
        WM_POWERBROADCAST => {
            if w == 7 || w == 18 {
                refresh(app);
            }
            1
        }
        WM_CLOSE => {
            app.visible = false;
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_ENDSESSION => {
            if w != 0 {
                save_position(app);
            }
            0
        }
        WM_DESTROY => {
            app.closing = true;
            KillTimer(hwnd, 1);
            Shell_NotifyIconW(NIM_DELETE, &app.tray);
            if !app.settings.is_null() {
                DestroyWindow(app.settings);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe fn show_widget(app: &mut App) {
    app.visible = true;
    SetWindowPos(
        app.widget,
        if app.config.always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        },
        0,
        0,
        app.size,
        app.size,
        SWP_NOMOVE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    ShowWindow(app.widget, SW_SHOWNA);
}
unsafe fn restore_position(app: &mut App) {
    let monitor = MonitorFromPoint(
        POINT {
            x: app.config.x,
            y: app.config.y,
        },
        MONITOR_DEFAULTTONEAREST,
    );
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..zeroed()
    };
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        let r = info.rcWork;
        let (x, y) = clamp_position(
            app.config.x,
            app.config.y,
            app.size,
            (r.left, r.top, r.right, r.bottom),
        );
        SetWindowPos(
            app.widget,
            null_mut(),
            x,
            y,
            app.size,
            app.size,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        app.config.x = x;
        app.config.y = y;
    }
}
unsafe fn save_position(app: &mut App) {
    let mut r: RECT = zeroed();
    if GetWindowRect(app.widget, &mut r) != 0 {
        app.config.x = r.left;
        app.config.y = r.top;
        if let Err(e) = app.config.save(&app.path) {
            show_error(&e.to_string());
        }
    }
}
unsafe fn drain_events(app: &mut App) {
    let events: Vec<_> = app
        .events
        .as_ref()
        .map(|r| r.try_iter().collect())
        .unwrap_or_default();
    for event in events {
        match event {
            Event::Started => app.monitor.started(),
            Event::Success(l, s) => app.monitor.succeeded(l, s),
            Event::Failed(f) => app.monitor.failed(f),
        }
    }
    paint_or_report(app);
    update_tray(app);
    update_status_control(app);
    write_diagnostics(app);
}
unsafe fn refresh(app: &mut App) {
    if !app.monitor.refreshing {
        app.monitor.started();
        paint_or_report(app);
        if let Some(worker) = &app.worker {
            let _ = worker.commands.send(Command::Refresh);
        }
    }
}
unsafe fn paint_or_report(app: &App) {
    if let Err(error) = present(app) {
        eprintln!("悬浮窗绘制失败：{error}");
    }
}
unsafe fn present(app: &App) -> Result<()> {
    let model = app
        .monitor
        .render_model(Utc::now(), app.config.interval_secs);
    let mut pixels = render_weekly_quota_base(&model);
    draw_native_labels(&mut pixels, &model)?;
    let pixels = resize_bgra(&pixels, OUTPUT_SIZE, app.size as usize);
    let dc = GetDC(null_mut());
    if dc.is_null() {
        return Err(fail("获取绘制设备失败"));
    }
    let mem = CreateCompatibleDC(dc);
    if mem.is_null() {
        ReleaseDC(null_mut(), dc);
        return Err(fail("创建设备失败"));
    }
    let mut bits: *mut c_void = null_mut();
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: app.size,
        biHeight: -app.size,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..zeroed()
    };
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(mem);
        ReleaseDC(null_mut(), dc);
        return Err(fail("创建位图失败"));
    }
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
    let previous = SelectObject(mem, bitmap);
    let mut r: RECT = zeroed();
    GetWindowRect(app.widget, &mut r);
    let pos = POINT {
        x: r.left,
        y: r.top,
    };
    let origin = POINT { x: 0, y: 0 };
    let size = SIZE {
        cx: app.size,
        cy: app.size,
    };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let success = UpdateLayeredWindow(
        app.widget, dc, &pos, &size, mem, &origin, 0, &blend, ULW_ALPHA,
    );
    SelectObject(mem, previous);
    DeleteObject(bitmap);
    DeleteDC(mem);
    ReleaseDC(null_mut(), dc);
    if success == 0 {
        return Err(fail("更新悬浮窗失败"));
    }
    Ok(())
}
fn resize_bgra(source: &[u8], old: usize, new: usize) -> Vec<u8> {
    if old == new {
        return source.to_vec();
    }
    let mut out = vec![0; new * new * 4];
    for y in 0..new {
        for x in 0..new {
            let fx =
                ((x as f64 + 0.5) * old as f64 / new as f64 - 0.5).clamp(0.0, (old - 1) as f64);
            let fy =
                ((y as f64 + 0.5) * old as f64 / new as f64 - 0.5).clamp(0.0, (old - 1) as f64);
            let ix = fx.floor() as usize;
            let iy = fy.floor() as usize;
            let dx = fx - ix as f64;
            let dy = fy - iy as f64;
            for channel in 0..4 {
                let v = |xx, yy| f64::from(source[(yy * old + xx) * 4 + channel]);
                out[(y * new + x) * 4 + channel] =
                    ((v(ix, iy) * (1.0 - dx) + v((ix + 1).min(old - 1), iy) * dx) * (1.0 - dy)
                        + (v(ix, (iy + 1).min(old - 1)) * (1.0 - dx)
                            + v((ix + 1).min(old - 1), (iy + 1).min(old - 1)) * dx)
                            * dy)
                        .round() as u8;
            }
        }
    }
    out
}
unsafe fn draw_native_labels(base: &mut [u8], model: &WeeklyQuotaRenderModel) -> Result<()> {
    const SCALE: usize = 4;
    let side = (OUTPUT_SIZE * SCALE) as i32;
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return Err(fail("创建字体画布失败"));
    }
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: side,
        biHeight: -side,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..zeroed()
    };
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(dc);
        return Err(fail("创建字体位图失败"));
    }
    let previous = SelectObject(dc, bitmap);
    std::ptr::write_bytes(bits, 0, (side * side * 4) as usize);
    SetTextColor(dc, rgb(255, 255, 255));
    SetBkMode(dc, TRANSPARENT as i32);
    for (text, px, top, bottom, weight) in [
        (&model.percent_label, 30, 27, 64, 600),
        (&model.refresh_label, 11, 68, 88, 400),
    ] {
        let font = CreateFontW(
            -(px * SCALE as i32),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Microsoft YaHei UI").as_ptr(),
        );
        let old = SelectObject(dc, font);
        let mut r = RECT {
            left: 28 * SCALE as i32,
            top: top * SCALE as i32,
            right: 104 * SCALE as i32,
            bottom: bottom * SCALE as i32,
        };
        DrawTextW(
            dc,
            wide(text).as_ptr(),
            -1,
            &mut r,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old);
        DeleteObject(font);
    }
    GdiFlush();
    let mask = std::slice::from_raw_parts(bits as *const u8, (side * side * 4) as usize);
    for y in 0..OUTPUT_SIZE {
        for x in 0..OUTPUT_SIZE {
            let mut total = 0u32;
            for sy in 0..SCALE {
                for sx in 0..SCALE {
                    total +=
                        u32::from(mask[((y * SCALE + sy) * side as usize + x * SCALE + sx) * 4]);
                }
            }
            let alpha = total / 16;
            let i = (y * OUTPUT_SIZE + x) * 4;
            for (channel, color) in [211u32, 231, 255].into_iter().enumerate() {
                base[i + channel] =
                    ((color * alpha + u32::from(base[i + channel]) * (255 - alpha)) / 255) as u8;
            }
            base[i + 3] = (alpha + u32::from(base[i + 3]) * (255 - alpha) / 255).min(255) as u8;
        }
    }
    SelectObject(dc, previous);
    DeleteObject(bitmap);
    DeleteDC(dc);
    Ok(())
}
unsafe fn install_tray(app: &mut App) {
    app.tray = zeroed();
    app.tray.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    app.tray.hWnd = app.widget;
    app.tray.uID = 1;
    app.tray.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    app.tray.uCallbackMessage = TRAY;
    app.tray.hIcon = LoadIconW(GetModuleHandleW(null()), std::ptr::without_provenance(1));
    if app.tray.hIcon.is_null() {
        app.tray.hIcon = LoadIconW(null_mut(), IDI_INFORMATION);
    }
    set_tip(app);
    Shell_NotifyIconW(NIM_ADD, &app.tray);
}
unsafe fn set_tip(app: &mut App) {
    app.tray.szTip.fill(0);
    let text = format!(
        "{TITLE}\n{}",
        app.monitor.details(Utc::now(), app.config.interval_secs)
    );
    for (to, from) in app.tray.szTip.iter_mut().take(127).zip(text.encode_utf16()) {
        *to = from;
    }
}
unsafe fn update_tray(app: &mut App) {
    set_tip(app);
    Shell_NotifyIconW(NIM_MODIFY, &app.tray);
}
unsafe fn menu(app: &mut App) {
    let menu = CreatePopupMenu();
    for (id, label, flags) in [
        (
            REFRESH,
            "立即刷新",
            if app.monitor.refreshing {
                MF_GRAYED
            } else {
                MF_STRING
            },
        ),
        (OPEN_SETTINGS, "设置", MF_STRING),
        (
            TOP,
            "保持置顶",
            if app.config.always_on_top {
                MF_CHECKED
            } else {
                MF_STRING
            },
        ),
        (
            HIDE,
            if app.visible {
                "隐藏悬浮窗"
            } else {
                "显示悬浮窗"
            },
            MF_STRING,
        ),
        (EXIT, "退出", MF_STRING),
    ] {
        AppendMenuW(menu, flags, id, wide(label).as_ptr());
    }
    let mut cursor: POINT = zeroed();
    GetCursorPos(&mut cursor);
    SetForegroundWindow(app.widget);
    let choice = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        cursor.x,
        cursor.y,
        0,
        app.widget,
        null(),
    );
    DestroyMenu(menu);
    if choice > 0 {
        command(app, choice as usize);
    }
    PostMessageW(app.widget, WM_NULL, 0, 0);
}
unsafe fn command(app: &mut App, id: usize) {
    match id {
        REFRESH => refresh(app),
        OPEN_SETTINGS => open_settings(app),
        TOP => {
            let mut c = app.config.clone();
            c.always_on_top = !c.always_on_top;
            if apply_config(app, c) {
                show_widget(app);
                rebuild_settings(app);
            }
        }
        HIDE => {
            if app.visible {
                app.visible = false;
                ShowWindow(app.widget, SW_HIDE);
            } else {
                show_widget(app);
            }
        }
        EXIT => {
            save_position(app);
            DestroyWindow(app.widget);
        }
        _ => {}
    }
}
unsafe fn write_diagnostics(app: &App) {
    // Local, minimal and credential-free diagnostics; not an external log or an API payload.
    let payload = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"commit":env!("BUILD_COMMIT"),"dirty":env!("BUILD_DIRTY"),"status":app.monitor.details(Utc::now(),app.config.interval_secs),"remaining_percent":app.monitor.render_model(Utc::now(),app.config.interval_secs).remaining_percent,"refreshing":app.monitor.refreshing,"x":app.config.x,"y":app.config.y,"dpi":app.dpi,"interval_secs":app.config.interval_secs});
    if let Some(parent) = app.path.parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write(parent.join("diagnostics.json"), payload.to_string());
    }
}

unsafe fn open_settings(app: &mut App) {
    if !app.settings.is_null() {
        ShowWindow(app.settings, SW_RESTORE);
        SetForegroundWindow(app.settings);
        return;
    }
    let dpi = GetDpiForWindow(app.widget).max(96);
    let width = mul_div(560, dpi as i32, 96);
    let height = mul_div(560, dpi as i32, 96);
    app.settings = CreateWindowExW(
        WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
        wide(SETTINGS).as_ptr(),
        wide("南枫 Codex 额度 · 设置").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        width,
        height,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        (app as *mut App).cast(),
    );
    rebuild_settings(app);
    ShowWindow(app.settings, SW_SHOW);
    SetForegroundWindow(app.settings);
}
#[allow(clippy::too_many_arguments)] // Mirrors Win32 control placement with an explicit rectangle.
unsafe fn control(
    app: &mut App,
    class: &str,
    text: &str,
    id: usize,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let dpi = GetDpiForWindow(app.settings).max(96);
    let scale = |v| mul_div(v, dpi as i32, 96);
    let hwnd = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        scale(x),
        scale(y),
        scale(w),
        scale(h),
        app.settings,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(hwnd, WM_SETFONT, app.font as usize, 1);
    app.controls.push(hwnd);
    hwnd
}
unsafe fn rebuild_settings(app: &mut App) {
    if app.settings.is_null() {
        return;
    }
    for hwnd in app.controls.drain(..) {
        DestroyWindow(hwnd);
    }
    let dpi = GetDpiForWindow(app.settings).max(96);
    DeleteObject(app.font);
    DeleteObject(app.title_font);
    app.font = make_font(14, 400, dpi);
    app.title_font = make_font(22, 600, dpi);
    for (id, text, x) in [
        (T_GENERAL, "常规", 24),
        (T_REVIEW, "功能审阅", 196),
        (T_ABOUT, "关于", 368),
    ] {
        control(
            app,
            "BUTTON",
            text,
            id,
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            x,
            20,
            148,
            34,
        );
    }
    match app.page {
        T_GENERAL => {
            control(app, "STATIC", "额度状态", 0, 0, 28, 76, 470, 24);
            control(
                app,
                "STATIC",
                &app.monitor.details(Utc::now(), app.config.interval_secs),
                400,
                0,
                28,
                106,
                470,
                112,
            );
            control(app, "STATIC", "刷新间隔", 0, 0, 28, 230, 170, 24);
            let combo = control(
                app,
                "COMBOBOX",
                "",
                INTERVAL,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                204,
                224,
                180,
                200,
            );
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
                .position(|v| *v == app.config.interval_secs)
                .unwrap_or(1);
            SendMessageW(combo, CB_SETCURSEL, index, 0);
            let top = control(
                app,
                "BUTTON",
                "保持悬浮窗置顶",
                TOP_CHECK,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                28,
                278,
                420,
                26,
            );
            SendMessageW(top, BM_SETCHECK, usize::from(app.config.always_on_top), 0);
            let startup = control(
                app,
                "BUTTON",
                "登录 Windows 时启动",
                STARTUP,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                28,
                316,
                420,
                26,
            );
            SendMessageW(
                startup,
                BM_SETCHECK,
                usize::from(app.config.start_at_login),
                0,
            );
            control(
                app,
                "STATIC",
                "设置即时生效。拖动圆窗调整位置，右键打开菜单。",
                0,
                0,
                28,
                365,
                470,
                40,
            );
            control(
                app,
                "BUTTON",
                "立即刷新",
                REFRESH,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                28,
                426,
                148,
                36,
            );
            control(
                app,
                "BUTTON",
                "找回悬浮窗",
                POSITION,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                204,
                426,
                148,
                36,
            );
        }
        T_REVIEW => {
            control(app, "STATIC", "已采用", 0, 0, 28, 80, 470, 26);
            control(
                app,
                "STATIC",
                "只读额度监控\n复用本机 Codex 登录，不创建对话、不调用模型。\n\n周剩余额度圆窗\n保留常驻圆窗；五小时窗口只在接口提供时列出。\n\n异常与恢复\n保留最近成功值，明确提示连接失败和数据过期。\n\n右键菜单及托盘\n刷新、设置、隐藏和退出均从这里进入。",
                0,
                0,
                28,
                118,
                470,
                290,
            );
            control(
                app,
                "STATIC",
                "暂不采用：自动激活、独立登录、上游覆盖更新。\n理由：只读目标明确，减少权限与维护负担。",
                0,
                0,
                28,
                422,
                470,
                56,
            );
        }
        T_ABOUT => {}
        _ => {}
    }
    InvalidateRect(app.settings, null(), 1);
}
unsafe fn update_status_control(app: &mut App) {
    if app.settings.is_null() || app.page != T_GENERAL {
        return;
    }
    let status = GetDlgItem(app.settings, 400);
    if !status.is_null() {
        SetWindowTextW(
            status,
            wide(&app.monitor.details(Utc::now(), app.config.interval_secs)).as_ptr(),
        );
    }
    let button = GetDlgItem(app.settings, REFRESH as i32);
    EnableWindow(button, i32::from(!app.monitor.refreshing));
}
unsafe fn apply_config(app: &mut App, new: Config) -> bool {
    let old = app.config.clone();
    let startup_changed = old.start_at_login != new.start_at_login;
    if startup_changed && let Err(e) = set_startup(new.start_at_login) {
        show_error(&format!("无法更改开机启动：{e}"));
        return false;
    }
    if let Err(e) = new.save(&app.path) {
        if startup_changed {
            let _ = set_startup(old.start_at_login);
        }
        show_error(&e.to_string());
        return false;
    }
    app.config = new;
    SetWindowPos(
        app.widget,
        if app.config.always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        },
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    if let Some(worker) = &app.worker {
        let _ = worker.commands.send(Command::Configure(app.config.clone()));
    }
    write_diagnostics(app);
    true
}
unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    attach(hwnd, msg, l);
    let Some(app) = app_from(hwnd) else {
        return DefWindowProcW(hwnd, msg, w, l);
    };
    match msg {
        WM_COMMAND => {
            let id = w & 0xffff;
            let notification = (w >> 16) & 0xffff;
            match id {
                T_GENERAL | T_REVIEW | T_ABOUT => {
                    app.page = id;
                    rebuild_settings(app);
                }
                REFRESH => refresh(app),
                POSITION => {
                    let mut p: POINT = zeroed();
                    GetCursorPos(&mut p);
                    app.config.x = p.x - 66;
                    app.config.y = p.y - 66;
                    restore_position(app);
                    show_widget(app);
                    save_position(app);
                }
                TOP_CHECK | STARTUP => {
                    let mut config = app.config.clone();
                    let checked = SendMessageW(GetDlgItem(hwnd, id as i32), BM_GETCHECK, 0, 0)
                        == BST_CHECKED as isize;
                    if id == TOP_CHECK {
                        config.always_on_top = checked;
                    } else {
                        config.start_at_login = checked;
                    }
                    if !apply_config(app, config) {
                        rebuild_settings(app);
                    }
                }
                INTERVAL if notification == CBN_SELCHANGE as usize => {
                    let index = SendMessageW(GetDlgItem(hwnd, INTERVAL as i32), CB_GETCURSEL, 0, 0);
                    if (0..5).contains(&index) {
                        let mut config = app.config.clone();
                        config.interval_secs = [30, 60, 120, 300, 600][index as usize];
                        if !apply_config(app, config) {
                            rebuild_settings(app);
                        }
                    }
                }
                _ => {}
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkMode(w as HDC, TRANSPARENT as i32);
            SetTextColor(w as HDC, rgb(25, 39, 51));
            app.background as isize
        }
        WM_ERASEBKGND => {
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            FillRect(w as HDC, &r, app.background);
            1
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            if app.page == T_ABOUT {
                paint_about(app, dc);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_DPICHANGED => {
            let r = &*(l as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            rebuild_settings(app);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            app.controls.clear();
            app.settings = null_mut();
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe fn paint_about(app: &App, dc: HDC) {
    let dpi = GetDpiForWindow(app.settings).max(96);
    let s = |v| mul_div(v, dpi as i32, 96);
    let white = CreateSolidBrush(rgb(255, 255, 255));
    let old_brush = SelectObject(dc, white);
    let pen = CreatePen(PS_SOLID, 1, rgb(229, 234, 238));
    let old_pen = SelectObject(dc, pen);
    RoundRect(dc, s(24), s(110), s(516), s(490), s(16), s(16));
    SelectObject(dc, old_brush);
    DeleteObject(white);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, rgb(25, 39, 51));
    SelectObject(dc, app.title_font);
    let mut title = RECT {
        left: s(24),
        top: s(73),
        right: s(516),
        bottom: s(106),
    };
    DrawTextW(
        dc,
        wide("关于").as_ptr(),
        -1,
        &mut title,
        DT_CENTER | DT_SINGLELINE,
    );
    SelectObject(dc, app.font);
    let commit = env!("BUILD_COMMIT");
    let commit = &commit[..commit.len().min(12)];
    let suffix = if env!("BUILD_DIRTY") == "true" {
        "（本地改动）"
    } else {
        ""
    };
    for (y, h, text) in [
        (
            128,
            80,
            "产品信息\n南枫 Codex 额度\nWindows 只读额度悬浮窗，复用本机 Codex 登录。".to_string(),
        ),
        (
            235,
            106,
            format!(
                "版本信息\nDesktop 版 {} · 开发日期 2026-10-08\nGit Commit：{commit}{suffix}\nGitHub · nanzhufeng/NanfengCodexQuota-Windows",
                env!("CARGO_PKG_VERSION")
            ),
        ),
        (
            364,
            108,
            "开发者信息\n开发者：席瑞\n联系邮箱：nanzhufeng.studio@gmail.com\n版权所有 © 2026 席瑞"
                .to_string(),
        ),
    ] {
        let mut rect = RECT {
            left: s(44),
            top: s(y),
            right: s(496),
            bottom: s(y + h),
        };
        DrawTextW(
            dc,
            wide(&text).as_ptr(),
            -1,
            &mut rect,
            DT_LEFT | DT_WORDBREAK,
        );
    }
    for y in [219, 351] {
        MoveToEx(dc, s(44), s(y), null_mut());
        LineTo(dc, s(496), s(y));
    }
    SelectObject(dc, old_pen);
    DeleteObject(pen);
}
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "NanfengCodexQuota";
unsafe fn set_startup(enabled: bool) -> Result<()> {
    let mut key: HKEY = null_mut();
    let result = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide(RUN_KEY).as_ptr(),
        0,
        null_mut(),
        0,
        KEY_SET_VALUE,
        null(),
        &mut key,
        null_mut(),
    );
    anyhow::ensure!(result == 0, "无法访问当前用户启动项");
    let result = if enabled {
        let path = std::env::current_exe()?;
        let value = wide(&format!("\"{}\"", path.display()));
        RegSetValueExW(
            key,
            wide(RUN_NAME).as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    } else {
        let r = RegDeleteValueW(key, wide(RUN_NAME).as_ptr());
        if r == ERROR_FILE_NOT_FOUND { 0 } else { r }
    };
    RegCloseKey(key);
    anyhow::ensure!(result == 0, "启动项变更失败 ({result})");
    Ok(())
}
unsafe fn startup_matches_current_exe() -> bool {
    let mut buffer = [0u16; 4096];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    if RegGetValueW(
        HKEY_CURRENT_USER,
        wide(RUN_KEY).as_ptr(),
        wide(RUN_NAME).as_ptr(),
        RRF_RT_REG_SZ,
        null_mut(),
        buffer.as_mut_ptr().cast(),
        &mut size,
    ) != 0
    {
        return false;
    }
    let n = buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len());
    let value = String::from_utf16_lossy(&buffer[..n]);
    std::env::current_exe()
        .ok()
        .is_some_and(|p| value == format!("\"{}\"", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_preserves_transparent_premultiplied_pixels() {
        let source = [0, 0, 0, 0, 100, 50, 25, 100, 0, 0, 0, 0, 100, 50, 25, 100];
        let resized = resize_bgra(&source, 2, 5);
        for p in resized.chunks_exact(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
        }
        assert_eq!(&resized[..4], &[0, 0, 0, 0]);
    }
}
