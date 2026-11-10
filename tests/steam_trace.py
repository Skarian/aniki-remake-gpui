"""Read-only Steam directory timeline. Never launches, focuses or terminates a game."""
import argparse, ctypes as c, json, os, time, winreg
from ctypes import wintypes as w
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--directory',required=True)
p.add_argument('--app-id',required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--seconds',type=float,default=180)
p.add_argument('--interval-ms',type=float,default=250)
args=p.parse_args()
if args.interval_ms <= 0: p.error('--interval-ms must be positive')
directory=os.path.normcase(os.path.realpath(args.directory)).rstrip('\\/')
k=c.WinDLL('kernel32',use_last_error=True)
u=c.WinDLL('user32',use_last_error=True)
class Entry(c.Structure):
    _fields_=[('size',w.DWORD),('usage',w.DWORD),('pid',w.DWORD),('heap',c.c_size_t),
              ('module',w.DWORD),('threads',w.DWORD),('parent',w.DWORD),
              ('priority',w.LONG),('flags',w.DWORD),('name',w.WCHAR*260)]
k.CreateToolhelp32Snapshot.argtypes=[w.DWORD,w.DWORD];k.CreateToolhelp32Snapshot.restype=w.HANDLE
k.Process32FirstW.argtypes=[w.HANDLE,c.POINTER(Entry)];k.Process32NextW.argtypes=[w.HANDLE,c.POINTER(Entry)]
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,c.POINTER(w.DWORD)]
k.ProcessIdToSessionId.argtypes=[w.DWORD,c.POINTER(w.DWORD)]
k.CloseHandle.argtypes=[w.HANDLE]
u.GetForegroundWindow.restype=w.HWND
u.GetWindowThreadProcessId.argtypes=[w.HWND,c.POINTER(w.DWORD)]

def snapshot():
    snap=k.CreateToolhelp32Snapshot(2,0)
    if snap==c.c_void_p(-1).value: return dict(error=c.get_last_error())
    entry=Entry();entry.size=c.sizeof(entry)
    matches=[];upstream=[];unknown=[];error=None
    try:
        ok=k.Process32FirstW(snap,c.byref(entry))
        while ok:
            sid=w.DWORD()
            if k.ProcessIdToSessionId(entry.pid,c.byref(sid)) and sid.value!=0:
                handle=k.OpenProcess(0x1000,False,entry.pid)
                path=None
                if handle:
                    try:
                        buf=c.create_unicode_buffer(32768);length=w.DWORD(len(buf))
                        if k.QueryFullProcessImageNameW(handle,0,buf,c.byref(length)): path=buf.value
                    finally: k.CloseHandle(handle)
                if path:
                    lower=os.path.normcase(path)
                    item=dict(pid=entry.pid,parent=entry.parent,path=path)
                    if directory in lower: upstream.append(item)
                    if lower.startswith(directory+'\\'): matches.append(item)
                else: unknown.append(entry.pid)
            ok=k.Process32NextW(snap,c.byref(entry))
        if c.get_last_error()!=18: error=c.get_last_error() # ERROR_NO_MORE_FILES
    finally: k.CloseHandle(snap)
    return dict(matched=matches,upstream=upstream,unknown_image_pids=unknown,error=error)

args.output.parent.mkdir(parents=True,exist_ok=True)
stop=args.output.with_suffix('.stop')
if stop.exists(): raise SystemExit('Use a fresh output name; stop marker already exists')
start=time.perf_counter();deadline=start
with args.output.open('x',encoding='utf-8') as out:
    out.write(json.dumps(dict(type='metadata',directory=directory,app_id=args.app_id,
                              started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),interval_ms=args.interval_ms))+'\n')
    while time.perf_counter()-start<args.seconds and not stop.exists():
        sampled=time.perf_counter();row=snapshot()
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER,'Software\\Valve\\Steam\\Apps\\'+args.app_id) as key:
                running=winreg.QueryValueEx(key,'Running')[0]
        except OSError: running=None
        foreground=u.GetForegroundWindow();pid=w.DWORD()
        u.GetWindowThreadProcessId(foreground,c.byref(pid))
        row.update(type='sample',elapsed_ms=round((sampled-start)*1000,3),
                   scan_ms=round((time.perf_counter()-sampled)*1000,3),steam_running=running,
                   foreground_hwnd=foreground,foreground_pid=pid.value)
        out.write(json.dumps(row)+'\n');out.flush()
        deadline+=args.interval_ms/1000
        time.sleep(max(0,deadline-time.perf_counter()))
print(str(args.output),flush=True)
