import os, sys
for root, _, fs in os.walk(sys.argv[1]):
    for f in fs:
        try:
            fd = os.open(os.path.join(root, f), os.O_RDONLY); os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED); os.close(fd)
        except OSError: pass
