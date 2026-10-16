"""Page-cache control for a corpus saved by `foldbench save`.

usage: python3 -I cachectl.py DIR evict|resident|read

evict     fsync, then posix_fadvise(POSIX_FADV_DONTNEED) on every pack, pending file and
          index.bin. DONTNEED drops only clean pages, hence the fsync. It evicts the pages
          from this machine's page cache only; a virtual machine's host may still hold them.
resident  counts the bytes of those files that are in the page cache (mincore).
read      reads every file once and prints how long that took.
"""
import ctypes
import os
import sys
import time

root, action = sys.argv[1], sys.argv[2]
paths = []
for sub in ("packs", "pending"):
    d = os.path.join(root, sub)
    paths += [os.path.join(d, n) for n in sorted(os.listdir(d))]
index = os.path.join(root, "index.bin")
if os.path.exists(index):
    paths.append(index)

if action == "evict":
    for p in paths:
        fd = os.open(p, os.O_RDONLY)
        try:
            os.fsync(fd)
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        finally:
            os.close(fd)
elif action == "resident":
    libc = ctypes.CDLL("libc.so.6", use_errno=True)
    libc.mmap.restype = ctypes.c_void_p
    libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int,
                          ctypes.c_int, ctypes.c_long]
    libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_char_p]
    page = os.sysconf("SC_PAGE_SIZE")
    resident = total = 0
    for p in paths:
        size = os.path.getsize(p)
        total += size
        if size == 0:
            continue
        fd = os.open(p, os.O_RDONLY)
        try:
            addr = libc.mmap(None, size, 1, 1, fd, 0)  # PROT_READ, MAP_SHARED
            if addr in (None, ctypes.c_void_p(-1).value):
                raise OSError(ctypes.get_errno(), "mmap")
            pages = (size + page - 1) // page
            vec = ctypes.create_string_buffer(pages)
            if libc.mincore(addr, size, vec) != 0:
                raise OSError(ctypes.get_errno(), "mincore")
            resident += sum(b & 1 for b in vec.raw) * page
            libc.munmap(addr, size)
        finally:
            os.close(fd)
    print(f"resident {min(resident, total)} of {total} bytes")
elif action == "read":
    started = time.time()
    total = 0
    for p in paths:
        with open(p, "rb") as f:
            while True:
                chunk = f.read(1 << 20)
                if not chunk:
                    break
                total += len(chunk)
    print(f"read {total} bytes in {time.time() - started:.3f} s")
else:
    sys.exit("unknown action")
