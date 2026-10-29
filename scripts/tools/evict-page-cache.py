"""Drop a folder's files from the operating system's page cache.

Usage: python3 -I scripts/tools/evict-page-cache.py <dir>

A measurement of first-launch seeding (docs/evidence/first-launch-seeding-2026-10-18.md) reads
the bundled store once. Files that were just built or copied are still in memory, and reading
them says little about a person's first launch, when the installer's files are on the disk.
`posix_fadvise(DONTNEED)` asks the kernel to forget the clean pages of a file the caller can
open, so the next read goes to the disk. It needs no privilege. Directory entries and inodes
stay cached, and so do pages the kernel could not drop (dirty ones); `sync` first if the folder
was written a moment ago.

Prints the number of files and bytes it asked the kernel to forget.
"""
import os
import sys

root = sys.argv[1]
files = size = 0
for folder, _, names in os.walk(root):
    for name in names:
        path = os.path.join(folder, name)
        try:
            fd = os.open(path, os.O_RDONLY)
        except OSError:
            continue
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
            files += 1
            size += os.fstat(fd).st_size
        finally:
            os.close(fd)
print(f"asked the kernel to forget {files} files, {size} bytes under {root}")
