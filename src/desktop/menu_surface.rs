//! Compact native popup geometry adapted from the Mac context card.
use super::*;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

pub(super) const WIDTH: i32 = 190;
pub(super) const HEIGHT: i32 = 166;

pub(super) fn anchor(click: POINT, work: RECT, dpi: u32) -> POINT {
    let s = |n| mul_div(n, dpi as i32, 96);
    POINT {
        x: (click.x + s(8)).clamp(work.left, (work.right - s(WIDTH)).max(work.left)),
        y: (click.y - s(8) - s(HEIGHT)).clamp(work.top, (work.bottom - s(HEIGHT)).max(work.top)),
    }
}

pub(super) fn centered(width: i32, height: i32, work: RECT) -> RECT {
    let width = width.min(work.right - work.left).max(1);
    let height = height.min(work.bottom - work.top).max(1);
    let left = work.left + (work.right - work.left - width) / 2;
    let top = work.top + (work.bottom - work.top - height) / 2;
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// The hook applies only to this UI thread during TrackPopupMenu. Native menu
/// selection, dismissal, accessibility and capture remain owned by Windows.
pub(super) struct MenuHook(HHOOK);
impl MenuHook {
    pub(super) unsafe fn install() -> Self {
        Self(SetWindowsHookExW(
            WH_CBT,
            Some(created),
            null_mut(),
            GetCurrentThreadId(),
        ))
    }
}
impl Drop for MenuHook {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                UnhookWindowsHookEx(self.0);
            }
        }
    }
}
#[derive(Default)]
struct Frame {
    width: i32,
    height: i32,
    dpi: u32,
}
unsafe extern "system" fn created(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HCBT_CREATEWND as i32 {
        let hwnd = w as HWND;
        let mut name = [0u16; 32];
        let len = GetClassNameW(hwnd, name.as_mut_ptr(), name.len() as i32);
        if String::from_utf16_lossy(&name[..len.max(0) as usize]) == "#32768" {
            let frame = Box::into_raw(Box::new(Frame::default()));
            if SetWindowSubclass(hwnd, Some(frame_proc), 2, frame as usize) == 0 {
                drop(Box::from_raw(frame));
            }
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}
unsafe extern "system" fn frame_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    reference: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(frame_proc), 2);
        let result = DefSubclassProc(hwnd, msg, w, l);
        drop(Box::from_raw(reference as *mut Frame));
        return result;
    }
    let result = DefSubclassProc(hwnd, msg, w, l);
    if msg == WM_WINDOWPOSCHANGED && IsWindow(hwnd) != 0 {
        let mut rect: RECT = zeroed();
        GetWindowRect(hwnd, &mut rect);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let frame = &mut *(reference as *mut Frame);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width > 0 && height > 0 && (frame.width, frame.height, frame.dpi) != (width, height, dpi)
        {
            *frame = Frame { width, height, dpi };
            let diameter = mul_div(22, dpi as i32, 96);
            let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
            if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
                DeleteObject(region);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_uses_the_upper_right_anchor_and_stays_in_the_work_area() {
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let p = anchor(POINT { x: 500, y: 500 }, work, 96);
        assert_eq!((p.x, p.y), (508, 326));
        let p = anchor(POINT { x: 1918, y: 10 }, work, 96);
        assert_eq!((p.x, p.y), (1730, 0));
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let p = anchor(POINT { x: -1000, y: 600 }, work, 96);
        assert_eq!((p.x, p.y), (-992, 426));
    }
    #[test]
    fn main_window_centers_and_fits_smaller_or_negative_coordinate_screens() {
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let r = centered(1080, 750, work);
        assert_eq!((r.left, r.top, r.right, r.bottom), (-1500, 145, -420, 895));
        let r = centered(
            1080,
            750,
            RECT {
                left: 0,
                top: 40,
                right: 1024,
                bottom: 740,
            },
        );
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 40, 1024, 740));
    }
}
