"""Exercise topmost recovery and taskbar avoidance using a disposable local window."""
import argparse,ctypes as c,json,pathlib,time,tkinter as tk,os,re,tomllib
from ctypes import wintypes as w
p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
u=c.windll.user32;u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
for name,params,result in [('FindWindowW',[w.LPCWSTR,w.LPCWSTR],w.HWND),('GetWindow',[w.HWND,w.UINT],w.HWND),('GetForegroundWindow',[],w.HWND),('GetWindowRect',[w.HWND,c.POINTER(w.RECT)],w.BOOL),('SetWindowPos',[w.HWND,w.HWND,c.c_int,c.c_int,c.c_int,c.c_int,w.UINT],w.BOOL),('SendMessageW',[w.HWND,w.UINT,w.WPARAM,w.LPARAM],w.LPARAM),('PostMessageW',[w.HWND,w.UINT,w.WPARAM,w.LPARAM],w.BOOL),('SetForegroundWindow',[w.HWND],w.BOOL),('GetWindowLongPtrW',[w.HWND,c.c_int],w.LPARAM),('IsWindowVisible',[w.HWND],w.BOOL),('MonitorFromWindow',[w.HWND,w.DWORD],w.HANDLE)]:
 f=getattr(u,name);f.argtypes=params;f.restype=result
class MI(c.Structure):_fields_=[('size',w.DWORD),('monitor',w.RECT),('work',w.RECT),('flags',w.DWORD)]
u.GetMonitorInfoW.argtypes=[w.HANDLE,c.POINTER(MI)]
deadline=time.monotonic()+25
expected_version=re.search(r'^version = "([^"]+)"',(pathlib.Path(__file__).resolve().parents[1]/'Cargo.toml').read_text(encoding='utf-8'),re.M)[1]
while time.monotonic()<deadline:
 widget=u.FindWindowW('NanfengCodexQuota.Widget.v1',None)
 try: snapshot=json.loads((pathlib.Path(os.environ['LOCALAPPDATA'])/'NanfengCodexQuota/diagnostics.json').read_text(encoding='utf-8'))
 except (FileNotFoundError,json.JSONDecodeError): snapshot={}
 if widget and u.IsWindowVisible(widget) and snapshot.get('version')==expected_version and snapshot.get('refreshing') is False:break
 time.sleep(.05)
else:raise AssertionError('Application did not initialize')
config_path=pathlib.Path(os.environ['LOCALAPPDATA'])/'NanfengCodexQuota/settings.toml'
original_top=tomllib.loads(config_path.read_text(encoding='utf-8'))['always_on_top']
# The diagnostic file may belong to a previous run; wait for window initialization
# and compare against the user's preference, never infer it from transient styles.
deadline=time.monotonic()+5
while bool(u.GetWindowLongPtrW(widget,-20)&8)!=original_top and time.monotonic()<deadline:time.sleep(.05)
assert bool(u.GetWindowLongPtrW(widget,-20)&8)==original_top
orig=w.RECT();u.GetWindowRect(widget,c.byref(orig))
foreground=u.GetForegroundWindow();info=MI(size=c.sizeof(MI));assert u.GetMonitorInfoW(u.MonitorFromWindow(widget,2),c.byref(info))
root=None;report={}
def pause(seconds):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if root:root.update()
  time.sleep(.02)
def above(top,bottom):
 current=u.GetWindow(bottom,3)
 while current:
  if current==top:return True
  current=u.GetWindow(current,3)
 return False
try:
 if not original_top:u.SendMessageW(widget,0x111,102,0)
 x,y=info.work.left+200,info.work.top+200
 u.SetWindowPos(widget,w.HWND(-1),x,y,0,0,0x11)
 root=tk.Tk();root.overrideredirect(True);root.geometry(f'180x180+{x}+{y}');root.configure(bg='white');root.attributes('-topmost',True);root.update()
 blocker=u.GetForegroundWindow()
 # Tk's decorated wrapper is the parent of winfo_id even when borderless.
 u.GetParent.argtypes=[w.HWND];u.GetParent.restype=w.HWND
 blocker=u.GetParent(root.winfo_id()) or root.winfo_id()
 u.SetWindowPos(blocker,w.HWND(-1),x,y,180,180,0x50);u.SetForegroundWindow(blocker)
 expected_focus=u.GetForegroundWindow();pause(2.5)
 report['recovers_above_other_topmost']=above(widget,blocker)
 report['does_not_steal_focus']=u.GetForegroundWindow()==expected_focus
 # External style demotion must heal without changing the user's preference.
 u.SetWindowPos(widget,w.HWND(-2),0,0,0,0,0x13);pause(1.5)
 report['topmost_style_recovers']=bool(u.GetWindowLongPtrW(widget,-20)&8)
 # Never pull the widget out of an active drag before the user releases it.
 u.SendMessageW(widget,0x231,0,0)
 u.SetWindowPos(widget,None,x,info.monitor.bottom-20,0,0,0x15);pause(1.3)
 during=w.RECT();u.GetWindowRect(widget,c.byref(during))
 report['does_not_interrupt_drag']=during.top==info.monitor.bottom-20
 u.SendMessageW(widget,0x232,0,0)
 # Simulate a completed user drag into the bottom taskbar band.
 u.SetWindowPos(widget,None,x,info.monitor.bottom-20,0,0,0x15)
 u.SendMessageW(widget,0x232,0,0);pause(.15)
 r=w.RECT();u.GetWindowRect(widget,c.byref(r))
 report['drag_stays_inside_work_area']=r.bottom<=info.work.bottom and r.left>=info.work.left
 u.SendMessageW(widget,0x111,102,0)
 u.SetWindowPos(blocker,w.HWND(-1),r.left,r.top,180,180,0x50);pause(2.2)
 report['disabled_topmost_respected']=not(u.GetWindowLongPtrW(widget,-20)&8) and above(blocker,widget)
 u.SendMessageW(widget,0x111,102,0)
 u.SendMessageW(widget,0x111,104,0);pause(1.3)
 report['hidden_stays_hidden']=not u.IsWindowVisible(widget)
 u.SendMessageW(widget,0x111,104,0)
finally:
 u.SendMessageW(widget,0x232,0,0)
 if root:root.destroy()
 if tomllib.loads(config_path.read_text(encoding='utf-8'))['always_on_top']!=original_top:u.SendMessageW(widget,0x111,102,0)
 u.SetWindowPos(widget,None,orig.left,orig.top,0,0,0x15);u.SendMessageW(widget,0x232,0,0)
 if foreground:u.SetForegroundWindow(foreground)
(a.output/'topmost-qa.json').write_text(json.dumps(report,indent=2),encoding='utf-8');print(json.dumps(report));assert all(report.values()),report
