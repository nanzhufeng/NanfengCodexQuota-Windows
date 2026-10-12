"""Validate the application's native popup; restore pointer and cancel it on exit."""
import argparse
import ctypes as c
import json
import pathlib
import os
import re
import time
from ctypes import wintypes as w

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=pathlib.Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
u, g, k = c.windll.user32, c.windll.gdi32, c.windll.kernel32
u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
for dll, name, params, result in [
    (u, "FindWindowW", [w.LPCWSTR, w.LPCWSTR], w.HWND),
    (u, "GetWindowThreadProcessId", [w.HWND, c.POINTER(w.DWORD)], w.DWORD),
    (u, "PostMessageW", [w.HWND, w.UINT, w.WPARAM, w.LPARAM], w.BOOL),
    (u, "GetClassNameW", [w.HWND, w.LPWSTR, c.c_int], c.c_int),
    (u, "IsWindowVisible", [w.HWND], w.BOOL),
    (u, "GetWindowRect", [w.HWND, c.POINTER(w.RECT)], w.BOOL),
    (u, "GetDpiForWindow", [w.HWND], w.UINT),
    (u, "GetWindowRgn", [w.HWND, w.HANDLE], c.c_int),
    (g, "CreateRectRgn", [c.c_int, c.c_int, c.c_int, c.c_int], w.HANDLE),
    (g, "PtInRegion", [w.HANDLE, c.c_int, c.c_int], w.BOOL),
    (g, "DeleteObject", [w.HANDLE], w.BOOL),
    (k, "OpenProcess", [w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
    (k, "CloseHandle", [w.HANDLE], w.BOOL),
    (u, "GetGuiResources", [w.HANDLE, w.DWORD], w.DWORD),
    (u, "GetCursorPos", [c.POINTER(w.POINT)], w.BOOL),
]:
    fn = getattr(dll, name)
    fn.argtypes, fn.restype = params, result

hwnd = u.FindWindowW("NanfengCodexQuota.Widget.v1", None)
assert hwnd, "Application is not running"
pid = w.DWORD()
thread = u.GetWindowThreadProcessId(hwnd, c.byref(pid))
process = k.OpenProcess(0x400, False, pid.value)
callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u.EnumThreadWindows.argtypes = [w.DWORD, callback_type, w.LPARAM]

def popup():
    found = []
    @callback_type
    def collect(handle, _):
        name = c.create_unicode_buffer(32)
        u.GetClassNameW(handle, name, 32)
        if name.value == "#32768" and u.IsWindowVisible(handle):
            found.append(handle)
        return True
    u.EnumThreadWindows(thread, collect, 0)
    return found[0] if found else None

def wait(test):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        result = test()
        if result:
            return result
        time.sleep(0.02)
    raise AssertionError("Popup condition timed out")

def close():
    u.PostMessageW(hwnd, 0x1F, 0, 0)  # WM_CANCELMODE: native tracking cancellation
    wait(lambda: not popup())

def open_menu():
    u.PostMessageW(hwnd, 0x7B, 0, 0)  # WM_CONTEXTMENU: same user entry point
    return wait(popup)

original = w.POINT()
u.GetCursorPos(c.byref(original))
report = {}
try:
    # Initial live quota completion can replace a tray icon. Do not attribute
    # that one-time application allocation to opening and closing menus.
    expected_version = re.search(r'^version = "([^"]+)"', (pathlib.Path(__file__).resolve().parents[1] / "Cargo.toml").read_text(encoding="utf-8"), re.M)[1]
    diagnostics_path = pathlib.Path(os.environ["LOCALAPPDATA"]) / "NanfengCodexQuota" / "diagnostics.json"
    ready_by = time.monotonic() + 25
    while time.monotonic() < ready_by:
        try:
            snapshot = json.loads(diagnostics_path.read_text(encoding="utf-8"))
            if snapshot["version"] == expected_version and not snapshot["refreshing"]:
                break
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    else:
        raise AssertionError("Initial query did not settle")
    point = w.POINT()
    u.GetCursorPos(c.byref(point))
    menu = open_menu()
    rect = w.RECT()
    u.GetWindowRect(menu, c.byref(rect))
    dpi = u.GetDpiForWindow(menu)
    width, height = rect.right - rect.left, rect.bottom - rect.top
    assert abs(width * 96 / dpi - 190) <= 2, (width, height, dpi)
    assert abs(height * 96 / dpi - 166) <= 2, (width, height, dpi)
    class MonitorInfo(c.Structure):
        _fields_ = [("size", w.DWORD), ("monitor", w.RECT), ("work", w.RECT), ("flags", w.DWORD)]
    u.MonitorFromPoint.argtypes = [w.POINT, w.DWORD]
    u.MonitorFromPoint.restype = w.HANDLE
    u.GetMonitorInfoW.argtypes = [w.HANDLE, c.POINTER(MonitorInfo)]
    info = MonitorInfo(size=c.sizeof(MonitorInfo))
    assert u.GetMonitorInfoW(u.MonitorFromPoint(point, 2), c.byref(info))
    expected_x = max(info.work.left, min(point.x + round(8 * dpi / 96), info.work.right - width))
    expected_y = max(info.work.top, min(point.y - round(8 * dpi / 96) - height, info.work.bottom - height))
    assert abs(rect.left-expected_x) <= 2 and abs(rect.top-expected_y) <= 2
    report["upper_right_anchor_and_bounds"] = True
    region = g.CreateRectRgn(0, 0, 0, 0)
    try:
        assert u.GetWindowRgn(menu, region) > 0, "Popup has no rounded region"
        assert not g.PtInRegion(region, 0, 0), "Popup corner remains rectangular"
        assert g.PtInRegion(region, width // 2, height // 2)
    finally:
        g.DeleteObject(region)
    report["logical_size"] = [round(width * 96 / dpi), round(height * 96 / dpi)]
    report["rounded_surface"] = True
    # Native menu consumes Escape while tracking; no global keyboard input.
    u.PostMessageW(hwnd, 0x100, 0x1B, 0)
    wait(lambda: not popup())
    report["escape_dismiss"] = True
    resources = lambda: [u.GetGuiResources(process, 0), u.GetGuiResources(process, 1)]
    baseline = resources()
    for _ in range(30):
        open_menu()
        close()
    after = resources()
    assert after == baseline, (baseline, after)
    report["repeat_open_close"] = 30
    report["gdi_user_before"] = baseline
    report["gdi_user_after"] = after
finally:
    if popup():
        close()
    k.CloseHandle(process)
    u.SetCursorPos(original.x, original.y)
(args.output / "menu-qa.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report))
