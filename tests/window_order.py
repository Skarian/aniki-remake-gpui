"""Read-only z-order diagnostics for explicitly supplied process IDs."""
import ctypes as c,json,sys
u=c.WinDLL('user32');ids={int(x) for x in sys.argv[1:]}
u.GetForegroundWindow.restype=c.c_void_p
u.GetWindowLongW.argtypes=[c.c_void_p,c.c_int]
u.IsWindowVisible.argtypes=[c.c_void_p]
u.IsIconic.argtypes=[c.c_void_p]
u.GetWindowThreadProcessId.argtypes=[c.c_void_p,c.POINTER(c.c_ulong)]
u.GetWindowTextW.argtypes=[c.c_void_p,c.c_wchar_p,c.c_int]
u.GetClassNameW.argtypes=[c.c_void_p,c.c_wchar_p,c.c_int]
rows=[]
class GuiInfo(c.Structure):
    _fields_=[('size',c.c_ulong),('flags',c.c_ulong),('active',c.c_void_p),('focus',c.c_void_p),('capture',c.c_void_p),('menu',c.c_void_p),('move',c.c_void_p),('caret',c.c_void_p),('rect',c.c_long*4)]
u.GetGUIThreadInfo.argtypes=[c.c_ulong,c.POINTER(GuiInfo)]
@c.WINFUNCTYPE(c.c_bool,c.c_void_p,c.c_void_p)
def visit(hwnd,_):
    pid=c.c_ulong();tid=u.GetWindowThreadProcessId(hwnd,c.byref(pid))
    if pid.value in ids:
        title=c.create_unicode_buffer(256);u.GetWindowTextW(hwnd,title,256)
        cls=c.create_unicode_buffer(256);u.GetClassNameW(hwnd,cls,256)
        rows.append(dict(hwnd=hwnd,pid=pid.value,title=title.value,window_class=cls.value,visible=bool(u.IsWindowVisible(hwnd)),iconic=bool(u.IsIconic(hwnd)),topmost=bool(u.GetWindowLongW(hwnd,-20)&8),foreground=u.GetForegroundWindow()==hwnd))
        info=GuiInfo();info.size=c.sizeof(info)
        if u.GetGUIThreadInfo(tid,c.byref(info)):
            rows[-1]['input']={key:getattr(info,key) for key in ('active','focus','capture','menu','flags')}
    return True
u.EnumWindows(visit,None)
print(json.dumps(rows,indent=2))
