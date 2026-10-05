#!/usr/bin/env python3
"""Build-only zstd filter using the pinned builder's libzstd."""
import ctypes as c
import sys

lib = c.CDLL('libzstd.so.1')
lib.ZSTD_getFrameContentSize.argtypes = [c.c_void_p, c.c_size_t]
lib.ZSTD_getFrameContentSize.restype = c.c_ulonglong
lib.ZSTD_decompressBound.argtypes = [c.c_void_p, c.c_size_t]
lib.ZSTD_decompressBound.restype = c.c_ulonglong
lib.ZSTD_decompress.argtypes = [c.c_void_p, c.c_size_t, c.c_void_p, c.c_size_t]
lib.ZSTD_decompress.restype = c.c_size_t
lib.ZSTD_isError.argtypes = [c.c_size_t]
lib.ZSTD_isError.restype = c.c_uint
source = sys.stdin.buffer.read(128 * 1024 * 1024 + 1)
if len(source) > 128 * 1024 * 1024:
    sys.exit('Compressed build input exceeds limit')
size = lib.ZSTD_getFrameContentSize(source, len(source))
if size >= 2**64 - 2:
    size = lib.ZSTD_decompressBound(source, len(source))
if size > 512 * 1024 * 1024:
    sys.exit('Expanded build input exceeds limit')
output = c.create_string_buffer(size)
actual = lib.ZSTD_decompress(output, size, source, len(source))
if lib.ZSTD_isError(actual):
    sys.exit('Invalid zstd build input')
sys.stdout.buffer.write(output.raw[:actual])
