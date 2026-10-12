//! Native controls on a borderless card with a separately feathered alpha shadow.
use super::menu_surface::{HEIGHT, WIDTH};
use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
const PAD: i32 = 16;
const CARD: &str = "NanfengCodexQuota.Context.v1";
const SHADOW: &str = "NanfengCodexQuota.ContextShadow.v1";
const IDS: [usize; 5] = [REFRESH, OPEN_SETTINGS, TOP, HIDE, EXIT];
fn distance(x: f32, y: f32) -> f32 {
    let qx = (x - WIDTH as f32 / 2.0).abs() - (WIDTH as f32 / 2.0 - 11.0);
    let qy = (y - HEIGHT as f32 / 2.0).abs() - (HEIGHT as f32 / 2.0 - 11.0);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - 11.0
}
fn shadow_pixels(dpi: u32) -> (i32, i32, Vec<u8>) {
    let scale = dpi as f32 / 96.0;
    let width = mul_div(WIDTH + PAD * 2, dpi as i32, 96);
    let height = mul_div(HEIGHT + PAD * 2, dpi as i32, 96);
    let mut pixels = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let px = (x as f32 + 0.5) / scale - PAD as f32;
            let py = (y as f32 + 0.5) / scale - PAD as f32;
            if distance(px, py) > 0.0 {
                let d = distance(px, py - 2.0).max(0.0);
                pixels[((y * width + x) * 4 + 3) as usize] =
                    (36.0 * (-d * d / 40.5).exp()).round() as u8;
            }
        }
    }
    (width, height, pixels)
}
struct Popup {
    hwnd: HWND,
    shadow: HWND,
    buttons: Vec<HWND>,
    dpi: u32,
    visible: bool,
    hovered: Option<usize>,
    pressed: Option<usize>,
    choice: usize,
    done: bool,
    shown: bool,
}
impl Popup {
    fn s(&self, n: i32) -> i32 {
        mul_div(n, self.dpi as i32, 96)
    }
    fn row(&self, p: POINT) -> Option<usize> {
        IDS.iter().enumerate().find_map(|(i, _)| {
            let top = self.s(6 + i as i32 * 30 + if i == 4 { 4 } else { 0 });
            (p.x >= self.s(6) && p.x < self.s(WIDTH - 6) && p.y >= top && p.y < top + self.s(28))
                .then_some(i)
        })
    }
    unsafe fn repaint(&self) {
        for &button in &self.buttons {
            InvalidateRect(button, null(), 0);
        }
    }
    unsafe fn focused(&self) -> Option<usize> {
        self.buttons.iter().position(|h| *h == GetFocus())
    }
    unsafe fn focus(&self, i: usize) {
        SetFocus(self.buttons[i]);
        self.repaint();
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            if GetCapture() == self.hwnd {
                ReleaseCapture();
            }
            if IsWindow(self.shadow) != 0 {
                DestroyWindow(self.shadow);
            }
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
unsafe fn register(name: &str, proc: WNDPROC) -> bool {
    let instance = GetModuleHandleW(null());
    let mut old: WNDCLASSW = zeroed();
    let name = wide(name);
    if GetClassInfoW(instance, name.as_ptr(), &mut old) != 0 {
        return true;
    }
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: instance,
        lpszClassName: name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..zeroed()
    }) != 0
}
unsafe fn show_shadow(state: &Popup, position: POINT) -> bool {
    let (width, height, pixels) = shadow_pixels(state.dpi);
    let screen = GetDC(null_mut());
    let dc = CreateCompatibleDC(screen);
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..zeroed()
    };
    let mut bits = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        return false;
    }
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
    let old = SelectObject(dc, bitmap);
    let destination = POINT {
        x: position.x - state.s(PAD),
        y: position.y - state.s(PAD),
    };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let origin = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let ok = UpdateLayeredWindow(
        state.shadow,
        screen,
        &destination,
        &size,
        dc,
        &origin,
        0,
        &blend,
        ULW_ALPHA,
    ) != 0;
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    ReleaseDC(null_mut(), screen);
    if ok {
        SetWindowPos(
            state.shadow,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    ok
}
pub(super) unsafe fn track(
    owner: HWND,
    position: POINT,
    dpi: u32,
    visible: bool,
    checked: bool,
) -> usize {
    if !register(CARD, Some(card_proc)) || !register(SHADOW, Some(DefWindowProcW)) {
        return 0;
    }
    let previous = GetForegroundWindow();
    let mut state = Box::new(Popup {
        hwnd: null_mut(),
        shadow: null_mut(),
        buttons: Vec::new(),
        dpi,
        visible,
        hovered: None,
        pressed: None,
        choice: 0,
        done: false,
        shown: false,
    });
    let ptr = (&mut *state) as *mut Popup;
    state.hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_CONTROLPARENT,
        wide(CARD).as_ptr(),
        wide("南枫 Codex 额度菜单").as_ptr(),
        WS_POPUP | WS_CLIPCHILDREN,
        position.x,
        position.y,
        state.s(WIDTH),
        state.s(HEIGHT),
        owner,
        null_mut(),
        GetModuleHandleW(null()),
        ptr.cast(),
    );
    if state.hwnd.is_null() {
        return 0;
    }
    let region = CreateRoundRectRgn(
        0,
        0,
        state.s(WIDTH) + 1,
        state.s(HEIGHT) + 1,
        state.s(22),
        state.s(22),
    );
    if !region.is_null() && SetWindowRgn(state.hwnd, region, 1) == 0 {
        DeleteObject(region);
    }
    for (i, (id, label)) in IDS
        .into_iter()
        .zip([
            "立即刷新",
            "打开主界面",
            "保持置顶",
            if visible {
                "隐藏悬浮窗"
            } else {
                "显示悬浮窗"
            },
            "退出",
        ])
        .enumerate()
    {
        let button = CreateWindowExW(
            0,
            wide("BUTTON").as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | if id == TOP {
                    BS_AUTOCHECKBOX as u32
                } else {
                    BS_PUSHBUTTON as u32
                },
            state.s(6),
            state.s(6 + i as i32 * 30 + if i == 4 { 4 } else { 0 }),
            state.s(WIDTH - 12),
            state.s(28),
            state.hwnd,
            id as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SetWindowSubclass(button, Some(row_proc), 3, ptr as usize);
        if id == TOP {
            SendMessageW(button, BM_SETCHECK, usize::from(checked), 0);
        }
        state.buttons.push(button);
    }
    state.shadow = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
        wide(SHADOW).as_ptr(),
        null(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        state.hwnd,
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if state.shadow.is_null() || !show_shadow(&state, position) {
        return 0;
    }
    ShowWindow(state.hwnd, SW_SHOW);
    SetWindowPos(
        state.hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    );
    state.shown = true;
    SetForegroundWindow(state.hwnd);
    SetFocus(state.hwnd);
    SetCapture(state.hwnd);
    let mut message: MSG = zeroed();
    loop {
        if state.done {
            break;
        }
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result <= 0 {
            if result == 0 {
                PostQuitMessage(message.wParam as i32);
            }
            break;
        }
        if message.message == WM_KEYDOWN && message.wParam != VK_TAB as usize {
            SendMessageW(state.hwnd, WM_KEYDOWN, message.wParam, message.lParam);
        } else if IsDialogMessageW(state.hwnd, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    let restore = GetForegroundWindow() == state.hwnd;
    let choice = state.choice;
    drop(state);
    if restore && IsWindow(previous) != 0 {
        SetForegroundWindow(previous);
    }
    choice
}
unsafe extern "system" fn card_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Popup;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    let state = &mut *ptr;
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = if msg == WM_PAINT {
                BeginPaint(hwnd, &mut ps)
            } else {
                w as HDC
            };
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            FillRect(dc, &r, GetStockObject(WHITE_BRUSH) as HBRUSH);
            let pen = CreatePen(PS_SOLID, 1, rgb(225, 229, 231));
            let old = SelectObject(dc, pen);
            MoveToEx(dc, state.s(12), state.s(127), null_mut());
            LineTo(dc, state.s(WIDTH - 12), state.s(127));
            SelectObject(dc, old);
            DeleteObject(pen);
            if msg == WM_PAINT {
                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN => {
            let point = POINT {
                x: (l as u16 as i16) as i32,
                y: ((l >> 16) as u16 as i16) as i32,
            };
            let index = state.row(point);
            if msg == WM_MOUSEMOVE {
                if state.hovered != index {
                    state.hovered = index;
                    if let Some(i) = index {
                        state.focus(i);
                    }
                    state.repaint();
                }
            } else if msg == WM_LBUTTONDOWN {
                state.pressed = index;
                if index.is_none() {
                    state.done = true;
                }
                state.repaint();
            } else if msg == WM_LBUTTONUP {
                if let Some(i) = index.filter(|i| Some(*i) == state.pressed) {
                    state.choice = IDS[i];
                    state.done = true;
                }
                state.pressed = None;
                state.repaint();
            } else {
                state.done = true;
            }
            0
        }
        WM_COMMAND => {
            let id = w & 0xffff;
            if IDS.contains(&id) {
                state.choice = id;
                state.done = true;
            }
            0
        }
        WM_KEYDOWN => {
            let current = state.focused();
            match w as u16 {
                VK_ESCAPE => state.done = true,
                VK_UP => state.focus(current.map(|i| (i + 4) % 5).unwrap_or(4)),
                VK_DOWN => state.focus(current.map(|i| (i + 1) % 5).unwrap_or(0)),
                VK_HOME => state.focus(0),
                VK_END => state.focus(4),
                VK_RETURN | VK_SPACE => {
                    state.choice = IDS[current.unwrap_or(0)];
                    state.done = true;
                }
                _ => {}
            }
            0
        }
        WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize && state.shown => {
            state.done = true;
            0
        }
        WM_CANCELMODE | WM_CLOSE | WM_DESTROY | WM_DPICHANGED => {
            state.done = true;
            0
        }
        WM_CAPTURECHANGED if state.shown && l as HWND != hwnd => {
            state.done = true;
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe extern "system" fn row_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    reference: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(row_proc), 3);
        return DefSubclassProc(hwnd, msg, w, l);
    }
    let state = &*(reference as *const Popup);
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = if msg == WM_PAINT {
                BeginPaint(hwnd, &mut ps)
            } else {
                w as HDC
            };
            let id = GetDlgCtrlID(hwnd) as usize;
            let index = IDS.iter().position(|v| *v == id);
            let mut rect: RECT = zeroed();
            GetClientRect(hwnd, &mut rect);
            let selected = state.hovered == index || GetFocus() == hwnd || state.pressed == index;
            let checked =
                id == TOP && SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
            let item = DRAWITEMSTRUCT {
                itemID: id as u32,
                hDC: dc,
                rcItem: rect,
                itemState: if selected { ODS_SELECTED } else { 0 }
                    | if checked {
                        windows_sys::Win32::UI::Controls::ODS_CHECKED
                    } else {
                        0
                    },
                ..zeroed()
            };
            settings_view::draw_menu(state.dpi, state.visible, &item);
            if msg == WM_PAINT {
                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_KEYDOWN
            if matches!(
                w as u16,
                VK_UP | VK_DOWN | VK_HOME | VK_END | VK_ESCAPE | VK_RETURN | VK_SPACE
            ) =>
        {
            SendMessageW(GetParent(hwnd), msg, w, l)
        }
        WM_SETFOCUS | WM_KILLFOCUS | BM_SETCHECK => {
            let result = DefSubclassProc(hwnd, msg, w, l);
            InvalidateRect(hwnd, null(), 0);
            result
        }
        _ => DefSubclassProc(hwnd, msg, w, l),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_has_a_smooth_zero_alpha_boundary_and_a_clear_center() {
        for dpi in [96, 144, 192] {
            let (w, h, p) = shadow_pixels(dpi);
            let alpha = |x: i32, y: i32| p[((y * w + x) * 4 + 3) as usize];
            for x in 0..w {
                assert_eq!(alpha(x, 0), 0);
                assert_eq!(alpha(x, h - 1), 0);
            }
            for y in 0..h {
                assert_eq!(alpha(0, y), 0);
                assert_eq!(alpha(w - 1, y), 0);
            }
            assert_eq!(alpha(w / 2, h / 2), 0);
            let right = mul_div(PAD + WIDTH, dpi as i32, 96);
            let mut last = 255;
            for x in right..w {
                let a = alpha(x, h / 2);
                assert!(a <= last && a <= 36);
                assert!(last == 255 || last - a <= 5);
                last = a;
            }
            assert_eq!(last, 0);
        }
    }
}
