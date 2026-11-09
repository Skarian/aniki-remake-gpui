"""Read-only Windows compositor observation used by the lifecycle test."""
import ctypes as c
from ctypes import wintypes as w
import time

kernel = c.WinDLL('kernel32', use_last_error=True)
kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
kernel.OpenProcess.restype = w.HANDLE
kernel.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
kernel.CloseHandle.argtypes = [w.HANDLE]
clock = c.WinDLL('dcomp').DCompositionWaitForCompositorClock
clock.argtypes = [w.UINT, c.c_void_p, w.DWORD]
clock.restype = w.DWORD


def sample(pid):
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return None
    try:
        created, exited, system, user = (w.FILETIME() for _ in range(4))
        if not kernel.GetProcessTimes(handle, c.byref(created), c.byref(exited), c.byref(system), c.byref(user)):
            return None
        created_ns = (((created.dwHighDateTime << 32) | created.dwLowDateTime) - 116444736000000000) * 100
        age = (time.time_ns() - created_ns) / 1e6
        before = time.perf_counter()
        status = clock(0, None, 17)
        return dict(pid=pid, process_ms=age, status=status,
                    call_ms=(time.perf_counter() - before) * 1000)
    finally:
        kernel.CloseHandle(handle)
