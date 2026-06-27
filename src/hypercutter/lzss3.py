"""
Copyright © magical

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
"""

# Changes from upstream:
# removed decompress
# removed decompress_file
# removed decompress_overlay
# removed main
# use shared DecompressionError from exceptions module
# catch StopIteration for truncated input
# restore size-mismatch check in LZSS10 path
# remove dead _overlay parameter

import logging
from collections.abc import Callable, Iterator
from struct import unpack
from typing import Final

from .exceptions import DecompressionError

logger = logging.getLogger(__name__)

LZSS10_HEADER: Final[int] = 0x10
LZSS11_HEADER: Final[int] = 0x11

__all__ = (
    "decompress_bytes",
    "DecompressionError",
)


def bits(byte: int) -> tuple[int, int, int, int, int, int, int, int]:
    return (
        (byte >> 7) & 1,
        (byte >> 6) & 1,
        (byte >> 5) & 1,
        (byte >> 4) & 1,
        (byte >> 3) & 1,
        (byte >> 2) & 1,
        (byte >> 1) & 1,
        (byte) & 1,
    )


def _make_decompress_helpers(
    data: bytearray, it: Iterator[int]
) -> tuple[
    Callable[[int], None], Callable[[], int], Callable[[], int], Callable[[], None]
]:
    """Create read/write helpers backed by a shared iterator and bytearray."""

    def writebyte(b: int) -> None:
        data.append(b)

    def readbyte() -> int:
        try:
            return next(it)
        except StopIteration:
            raise DecompressionError("truncated LZSS data: expected more bytes")

    def readshort() -> int:
        try:
            a = next(it)
            b = next(it)
        except StopIteration:
            raise DecompressionError("truncated LZSS data: expected more bytes")
        return (a << 8) | b

    def copybyte() -> None:
        try:
            data.append(next(it))
        except StopIteration:
            raise DecompressionError("truncated LZSS data: expected more bytes")

    return writebyte, readbyte, readshort, copybyte


def decompress_raw_lzss10(indata: bytes, decompressed_size: int) -> bytearray:
    """Decompress LZSS-compressed bytes. Returns a bytearray."""
    data = bytearray()
    it: Iterator[int] = iter(indata)
    writebyte, readbyte, readshort, copybyte = _make_decompress_helpers(data, it)

    while len(data) < decompressed_size:
        b = readbyte()
        flags = bits(b)
        for flag in flags:
            if flag == 0:
                copybyte()
            elif flag == 1:
                sh = readshort()
                count = (sh >> 0xC) + 3
                disp = (sh & 0xFFF) + 1

                for _ in range(count):
                    writebyte(data[-disp])
            else:
                raise DecompressionError(f"invalid LZSS flag: {flag}")

            if decompressed_size <= len(data):
                break

    if len(data) != decompressed_size:
        raise DecompressionError("decompressed size does not match the expected size")

    return data


def decompress_raw_lzss11(indata: bytes, decompressed_size: int) -> bytearray:
    """Decompress LZSS-compressed bytes. Returns a bytearray."""
    data = bytearray()
    it: Iterator[int] = iter(indata)
    writebyte, readbyte, readshort, copybyte = _make_decompress_helpers(data, it)

    while len(data) < decompressed_size:
        b = readbyte()
        flags = bits(b)
        for flag in flags:
            if flag == 0:
                copybyte()
            elif flag == 1:
                b = readbyte()
                indicator = b >> 4

                if indicator == 0:
                    # 8 bit count, 12 bit disp
                    # indicator is 0, don't need to mask b
                    count = b << 4
                    b = readbyte()
                    count += b >> 4
                    count += 0x11
                elif indicator == 1:
                    # 16 bit count, 12 bit disp
                    count = ((b & 0xF) << 12) + (readbyte() << 4)
                    b = readbyte()
                    count += b >> 4
                    count += 0x111
                else:
                    # indicator is count (4 bits), 12 bit disp
                    count = indicator
                    count += 1

                disp = ((b & 0xF) << 8) + readbyte()
                disp += 1

                try:
                    for _ in range(count):
                        writebyte(data[-disp])
                except IndexError:
                    raise DecompressionError(
                        f"invalid LZSS11 backreference: count={count} disp={disp} "
                        f"len_data={len(data)}"
                    )
            else:
                raise DecompressionError(f"invalid LZSS flag: {flag}")

            if decompressed_size <= len(data):
                break

    if len(data) != decompressed_size:
        raise DecompressionError("decompressed size does not match the expected size")

    return data


def decompress_bytes(data: bytes) -> bytearray:
    """Decompress LZSS-compressed bytes. Returns a bytearray."""
    header = data[:4]
    if header[0] == LZSS10_HEADER:
        decompress_raw: Callable[[bytes, int], bytearray] = decompress_raw_lzss10
    elif header[0] == LZSS11_HEADER:
        decompress_raw = decompress_raw_lzss11
    else:
        logger.warning("Invalid LZSS compression header: %02x", header[0])
        raise DecompressionError("not as lzss-compressed file")

    (decompressed_size,) = unpack("<L", header[1:] + b"\x00")

    compressed_data = data[4:]
    return decompress_raw(compressed_data, decompressed_size)
