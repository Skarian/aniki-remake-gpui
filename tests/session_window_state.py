"""Read-only Win32 checks of the native black cover and foreground ownership."""
import ctypes as c, json, sys
from ctypes import wintypes as w
from pathlib import Path
u=c.WinDLL('user32'); g=c.WinDLL('gdi32')
u.GetForegroundWindow.restype=w.HWND
u.GetWindow.argtypes=[w.HWND,w.UINT]; u.GetWindow.restype=w.HWND
u.GetClassLongPtrW.argtypes=[w.HWND,c.c_int]; u.GetClassLongPtrW.restype=c.c_size_t
u.GetWindowLongPtrW.argtypes=[w.HWND,c.c_int]; u.GetWindowLongPtrW.restype=c.c_ssize_t
u.GetWindowRect.argtypes=[w.HWND,c.POINTER(w.RECT)]
u.IsWindowVisible.argtypes=[w.HWND]
g.GetStockObject.argtypes=[c.c_int]; g.GetStockObject.restype=w.HANDLE
callback=c.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
windows=[]
@callback
def visit(hwnd,_):
    name=c.create_unicode_buffer(256); u.GetClassNameW(w.HWND(hwnd),name,256)
    if name.value=='AnikiSessionBlack': windows.append(hwnd)
    return True
u.EnumWindows(visit,0)
assert len(windows)==1, windows
cover=windows[0]; rect=w.RECT(); u.GetWindowRect(cover,c.byref(rect))
foreground=u.GetForegroundWindow()
above=[]; hwnd=cover
while hwnd:=u.GetWindow(hwnd,3): # GW_HWNDPREV
    if u.IsWindowVisible(hwnd): above.append(hwnd)
result={'cover':cover,'foreground':foreground,'cover_visible':bool(u.IsWindowVisible(cover)),
        'black_class_brush':u.GetClassLongPtrW(cover,-10)==g.GetStockObject(4),
        'no_activate':bool(u.GetWindowLongPtrW(cover,-20)&0x08000000),
        'rect':[rect.left,rect.top,rect.right,rect.bottom],
        'foreground_above_cover':foreground in above}
assert result['cover_visible'] and result['black_class_brush'] and not result['no_activate'],result
assert result['foreground_above_cover'],result
assert rect.right>rect.left and rect.bottom>rect.top,result
Path(sys.argv[1]).write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(result,indent=2))
