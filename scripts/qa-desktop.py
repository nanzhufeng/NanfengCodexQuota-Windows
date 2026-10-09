"""Exercise the real Windows UI; preserve settings and leave one application running."""
import argparse
import os
import ctypes as c
import json
import pathlib
import subprocess
import time
import winreg
from ctypes import wintypes as w

parser = argparse.ArgumentParser()
parser.add_argument("exe", type=pathlib.Path)
parser.add_argument("--skip-startup", action="store_true")
parser.add_argument("--output", type=pathlib.Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
u = c.windll.user32
k = c.windll.kernel32
u.FindWindowW.argtypes = [w.LPCWSTR, w.LPCWSTR]
u.FindWindowW.restype = w.HWND
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = w.LPARAM
u.GetDlgItem.argtypes = [w.HWND, c.c_int]
u.GetDlgItem.restype = w.HWND
u.IsWindowVisible.argtypes = [w.HWND]
u.GetWindowLongPtrW.argtypes = [w.HWND, c.c_int]
u.GetWindowLongPtrW.restype = w.LPARAM
u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenProcess.restype = w.HANDLE
k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
k.CloseHandle.argtypes = [w.HANDLE]
root = pathlib.Path(os.environ["LOCALAPPDATA"]) / "NanfengCodexQuota"
report = {}


def wait_until(test, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            value = test()
            if value:
                return value
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    raise AssertionError("condition was not met")


def widget():
    return u.FindWindowW("NanfengCodexQuota.Widget.v1", None)


def diagnostics():
    return json.loads((root / "diagnostics.json").read_text(encoding="utf-8"))


def command(hwnd, identifier):
    u.SendMessageW(hwnd, 0x111, identifier, 0)


def exit_application(hwnd):
    pid = w.DWORD()
    u.GetWindowThreadProcessId(hwnd, c.byref(pid))
    handle = k.OpenProcess(0x00100000, False, pid.value)
    try:
        command(hwnd, 103)
        assert k.WaitForSingleObject(handle, 5000) == 0, "application did not exit"
    finally:
        k.CloseHandle(handle)


def start():
    return subprocess.Popen([str(args.exe)], cwd=args.exe.parent, creationflags=0x08000000)


hwnd = wait_until(widget)
wait_until(lambda: not diagnostics()["refreshing"] and diagnostics()["remaining_percent"] is not None)
report["live_quota"] = diagnostics()["remaining_percent"]
existing = u.FindWindowW("NanfengCodexQuota.Settings.v1", None)
if existing:
    u.SendMessageW(existing, 0x10, 0, 0)
u.SendMessageW(hwnd, 0x8000 + 11, 0, 0x203)  # actual tray double-click callback
settings = wait_until(lambda: (s := u.FindWindowW("NanfengCodexQuota.Settings.v1", None)) and u.IsWindowVisible(s) and s)
assert not u.GetDlgItem(settings, 501), "Removed brand heading is still present"
u.SendMessageW(hwnd, 0x8000 + 11, 0, 0x203)
assert u.FindWindowW("NanfengCodexQuota.Settings.v1", None) == settings, "Double-click duplicated the main window"
report["tray_double_click"] = True
command(hwnd, 101)
settings = wait_until(lambda: u.FindWindowW("NanfengCodexQuota.Settings.v1", None))
command(settings, 301)
assert not u.GetDlgItem(settings, 302), "Removed feature-review navigation is still present"
command(settings, 302)
assert u.GetDlgItem(settings, 203), "Stale removed-page command changed the current view"
command(settings, 303)
assert not u.GetDlgItem(settings, 203), "About navigation did not switch pages"
assert u.GetDlgItem(settings, 701), "GitHub repository link is not an interactive control"
command(settings, 301)
assert u.GetDlgItem(settings, 203), "General navigation did not restore settings"
report["review_removed"] = True
report["navigation"] = True
top = u.GetDlgItem(settings, 203)
was_top = bool(u.SendMessageW(top, 0xF0, 0, 0))
u.SendMessageW(top, 0xF5, 0, 0)  # BM_CLICK: exercises the actual checkbox notification.
assert bool(u.GetWindowLongPtrW(hwnd, -20) & 8) != was_top
u.SendMessageW(top, 0xF5, 0, 0)
assert bool(u.GetWindowLongPtrW(hwnd, -20) & 8) == was_top
report["top_immediate"] = True

combo = u.GetDlgItem(settings, 201)
previous_index = u.SendMessageW(combo, 0x147, 0, 0)
u.SendMessageW(combo, 0x14E, 2, 0)
command(settings, (1 << 16) | 201)
assert diagnostics()["interval_secs"] == 120
u.SendMessageW(combo, 0x14E, previous_index, 0)
command(settings, (1 << 16) | 201)
report["interval_immediate"] = True

startup = u.GetDlgItem(settings, 202)
was_startup = bool(u.SendMessageW(startup, 0xF0, 0, 0))
run_key = r"Software\Microsoft\Windows\CurrentVersion\Run"


def startup_value():
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, run_key) as key:
        try:
            return winreg.QueryValueEx(key, "NanfengCodexQuota")[0]
        except FileNotFoundError:
            return None


if not args.skip_startup:
    u.SendMessageW(startup, 0xF5, 0, 0)
    assert bool(startup_value()) != was_startup
    u.SendMessageW(startup, 0xF5, 0, 0)
    assert bool(startup_value()) == was_startup
    report["startup_immediate"] = True

u.SendMessageW(settings, 0x10, 0, 0)
u.SendMessageW(hwnd, 0x10, 0, 0)
assert not u.IsWindowVisible(hwnd)
u.SendMessageW(hwnd, 0x8000 + 12, 0, 0)
assert u.IsWindowVisible(hwnd)
report["hide_restore"] = True

duplicate = start()
assert duplicate.wait(timeout=5) == 0
assert widget() == hwnd
report["single_instance"] = True

original = w.RECT()
u.GetWindowRect(hwnd, c.byref(original))
u.SetWindowPos(hwnd, None, 360, 240, 0, 0, 0x1 | 0x4 | 0x10)
u.SendMessageW(hwnd, 0x232, 0, 0)  # WM_EXITSIZEMOVE persists the moved position.
exit_application(hwnd)
start()
hwnd = wait_until(widget)
position = w.RECT()
u.GetWindowRect(hwnd, c.byref(position))
assert (position.left, position.top) == (360, 240)
u.SetWindowPos(hwnd, None, original.left, original.top, 0, 0, 0x1 | 0x4 | 0x10)
u.SendMessageW(hwnd, 0x232, 0, 0)
report["position_restart"] = True
report["clean_exit"] = True
wait_until(lambda: not diagnostics()["refreshing"] and diagnostics()["remaining_percent"] is not None)
report["final_build"] = diagnostics()["commit"]
report["dirty"] = diagnostics()["dirty"]
(args.output / "desktop-qa.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
print(json.dumps(report, ensure_ascii=True))
