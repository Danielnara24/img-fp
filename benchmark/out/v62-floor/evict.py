#!/usr/bin/env python3
"""evict.py DIR...: drop each file's pages from the page cache (posix_fadvise DONTNEED)."""
import os, sys
for root in sys.argv[1:]:
    for d, _, fs in os.walk(root):
        for f in fs:
            try:
                fd = os.open(os.path.join(d, f), os.O_RDONLY)
                os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED); os.close(fd)
            except OSError: pass
