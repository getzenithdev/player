"""Check every Mach-O slice's declared minimum OS; runnable on Linux or macOS.

This checks build metadata, not execution on an older operating system.
"""
import argparse
import struct
from pathlib import Path


def minimums(data, offset=0):
    magic = data[offset:offset + 4]
    if magic in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        wide = magic[-1] == 0xBF
        count = struct.unpack_from(">I", data, offset + 4)[0]
        for i in range(count):
            entry = offset + 8 + i * (32 if wide else 20)
            inner = struct.unpack_from(">Q" if wide else ">I", data, entry + 8)[0]
            yield from minimums(data, inner)
        return
    if magic != b"\xcf\xfa\xed\xfe":
        raise ValueError("Expected a 64-bit Mach-O executable")
    cpu, _, _, count = struct.unpack_from("<IIII", data, offset + 4)
    architecture = {0x01000007: "x86_64", 0x0100000C: "arm64"}.get(cpu, hex(cpu))
    current = offset + 32
    found = False
    for _ in range(count):
        command, size = struct.unpack_from("<II", data, current)
        if size < 8:
            raise ValueError("Invalid load command")
        if command in (0x24, 0x32):
            version = struct.unpack_from("<I", data, current + (12 if command == 0x32 else 8))[0]
            found = True
            yield architecture, (version >> 16, (version >> 8) & 255, version & 255)
        current += size
    if not found:
        raise ValueError("Missing minimum macOS version")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", required=True, choices=["x86_64", "arm64"])
    parser.add_argument("files", nargs="+", type=Path)
    args = parser.parse_args()
    for file in args.files:
        slices = list(minimums(file.read_bytes()))
        if args.arch not in [arch for arch, _ in slices]:
            raise SystemExit(f"{file}: missing {args.arch} slice")
        for arch, version in slices:
            print(f"{file.name}: {arch}, minimum macOS {'.'.join(map(str, version))}")
            if version > (12, 0, 0):
                raise SystemExit(f"{file}: minimum OS exceeds Monterey 12.0")


if __name__ == "__main__":
    main()
