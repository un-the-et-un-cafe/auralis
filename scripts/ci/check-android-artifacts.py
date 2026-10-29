#!/usr/bin/env python3
"""Fail-closed native packaging checks; not a substitute for 16-KB runtime tests.

Official reference: https://developer.android.com/guide/practices/page-sizes
Supply previously verified SDK zipalign / bundletool binaries; this script does
not download tools, sign packages, extract archive paths, or install applications.
AAB ZIP offsets are intentionally NOT checked: bundletool config governs APKs.
"""

import argparse
import json
from pathlib import Path, PurePosixPath
import stat
import struct
import subprocess
import sys
import zipfile

PAGE = 16384
MACHINES = {"arm64-v8a": 183, "x86_64": 62}
REQUIRED_LIBS = {"libomcp_ffi.so", "libjnidispatch.so"}
MAX_LIBRARY = 256 * 1024 * 1024


class InvalidArtifact(ValueError):
    """An artifact cannot establish the required compatibility invariants."""


def require(condition, message):
    if not condition:
        raise InvalidArtifact(message)


def check_elf(data, abi, name):
    """Inspect ELF64 little-endian program headers without executing the file."""
    require(len(data) >= 64, f"{name}: truncated ELF header")
    require(data[:7] == b"\x7fELF\x02\x01\x01", f"{name}: expected ELF64 little-endian v1")
    fields = struct.unpack_from("<HHIQQQIHHHHHH", data, 16)
    kind, machine, version, _, phoff, _, _, ehsize, phsize, phnum, _, _, _ = fields
    require(kind == 3 and version == 1, f"{name}: expected ET_DYN ELF v1")
    require(machine == MACHINES[abi], f"{name}: ABI/ELF machine mismatch")
    require(ehsize == 64 and phsize == 56, f"{name}: invalid ELF/program header size")
    require(0 < phnum < 65535, f"{name}: missing/extended program header count")
    require(phoff >= ehsize and phoff + phsize * phnum <= len(data),
            f"{name}: program headers outside file")
    loads = []
    relros = []
    for index in range(phnum):
        ptype, flags, offset, vaddr, _, filesz, memsz, align = struct.unpack_from(
            "<IIQQQQQQ", data, phoff + index * phsize)
        require(offset <= len(data) and filesz <= len(data) - offset,
                f"{name}: segment {index} outside file")
        if ptype == 1:  # PT_LOAD
            loads.append((flags, offset, vaddr, filesz, memsz))
            require(filesz <= memsz, f"{name}: PT_LOAD filesz exceeds memsz")
            require(align >= PAGE and align & (align - 1) == 0,
                    f"{name}: PT_LOAD alignment is not a power of two >= 16 KB")
            require(offset % align == vaddr % align,
                    f"{name}: PT_LOAD offset/address are incongruent")
            require(vaddr + memsz < 1 << 64, f"{name}: PT_LOAD address overflow")
        elif ptype == 0x6474E552:  # PT_GNU_RELRO
            require(vaddr + memsz < 1 << 64, f"{name}: GNU_RELRO address overflow")
            relros.append((offset, vaddr, filesz, memsz))
    require(loads, f"{name}: no PT_LOAD segments")
    for offset, start, filesz, memsz in relros:
        end = start + memsz
        if end % PAGE == 0:
            continue
        # Android's documented own-LOAD exception: rounding RELRO protection up
        # is safe only if that non-executable LOAD contains no writable tail
        # and no other writable/executable LOAD intersects the protected pages.
        # Bionic applies PROT_READ, removing both write and execute permissions. RELRO may include linker padding
        # beyond its LOAD's memory extent (e.g. AndroidX graphics-path 1.0.1).
        # Match file coverage as well as virtual start; unrelated/partial loads
        # must not satisfy this exception merely because their addresses fit.
        owners = [index for index, (flags, off, addr, size, memory) in enumerate(loads)
                  if flags & 2 and not flags & 1 and off == offset and addr == start and size == filesz
                  and 0 < memory <= memsz]
        require(len(owners) == 1, f"{name}: GNU_RELRO unaligned end has no exclusive own LOAD")
        protected_start = start - start % PAGE
        protected_end = (end + PAGE - 1) // PAGE * PAGE
        for index, (flags, _, addr, _, memory) in enumerate(loads):
            if index != owners[0] and flags & 3 and memory:
                require(addr + memory <= protected_start or addr >= protected_end,
                        f"{name}: GNU_RELRO rounded protection overlaps other writable/executable LOAD")
    return {"load_segments": len(loads), "relro_segments": len(relros)}


def valid_entry(info):
    name = info.filename
    # ZipInfo silently truncates names at NUL; use orig_filename too.
    require(name == info.orig_filename and "\\" not in name and "\x00" not in name,
            f"unsafe ZIP path: {name!r}")
    parts = name.rstrip("/").split("/")
    require(name and not name.startswith("/") and all(p not in ("", ".", "..") for p in parts),
            f"unsafe ZIP path: {name!r}")
    require(not PurePosixPath(name).is_absolute(), f"absolute ZIP path: {name!r}")
    require(not (info.flag_bits & 1), f"encrypted ZIP entry: {name}")
    mode = info.external_attr >> 16
    require(not stat.S_ISLNK(mode), f"symlink ZIP entry: {name}")


def local_data_offset(stream, info):
    stream.seek(info.header_offset)
    header = stream.read(30)
    require(len(header) == 30, f"{info.filename}: truncated local ZIP header")
    signature, _, flags, compression, _, _, _, _, _, namesize, extrasize = struct.unpack(
        "<IHHHHHIIIHH", header)
    require(signature == 0x04034B50, f"{info.filename}: invalid local ZIP header")
    require(flags == info.flag_bits and compression == info.compress_type,
            f"{info.filename}: inconsistent local ZIP flags/compression")
    raw_name = stream.read(namesize)
    expected = info.filename.encode("utf-8" if flags & 0x800 else "cp437")
    require(raw_name == expected, f"{info.filename}: inconsistent local ZIP name")
    return info.header_offset + 30 + namesize + extrasize


def native_location(name, kind):
    parts = name.split("/")
    if kind == "apk" and len(parts) == 3 and parts[0] == "lib":
        return parts[1], parts[2]
    if kind == "aab" and len(parts) == 4 and parts[1] == "lib":
        return parts[2], parts[3]
    return None


def inspect_archive(path, expected_abis, require_unsigned=False):
    """Pure parser checks, independently testable without SDK tooling."""
    path = Path(path)
    kind = path.suffix.lstrip(".")
    require(kind in ("apk", "aab"), f"{path}: expected .apk or .aab")
    require(not require_unsigned or kind == "apk", "--require-unsigned applies only to APK artifacts")
    libraries = []
    found = {abi: set() for abi in expected_abis}
    with zipfile.ZipFile(path) as archive, path.open("rb") as stream:
        if require_unsigned:
            # APK v2/v3 signing blocks end immediately before the central directory.
            # start_dir is zipfile's resolved offset, including a prepended ZIP stub.
            require(archive.start_dir >= 16, "truncated APK central directory offset")
            stream.seek(archive.start_dir - 16)
            require(stream.read(16) != b"APK Sig Block 42", "APK Signing Block present (v2/v3)")
        names = set()
        ranges = []
        for info in archive.infolist():
            valid_entry(info)
            require(info.filename not in names, f"duplicate ZIP entry: {info.filename}")
            names.add(info.filename)
            if require_unsigned:
                upper = info.filename.upper()
                require(not (upper.startswith("META-INF/") and
                             upper.endswith((".SF", ".RSA", ".DSA", ".EC"))),
                        f"APK v1 signature entry present: {info.filename}")
            location = native_location(info.filename, kind)
            if info.filename.endswith(".so"):
                require(location is not None, f"unexpected native library location: {info.filename}")
            if location is None or info.is_dir():
                continue
            abi, filename = location
            require(abi in expected_abis, f"unexpected ABI {abi}: {info.filename}")
            require(filename.endswith(".so"), f"unexpected native entry: {info.filename}")
            require(0 < info.file_size <= MAX_LIBRARY, f"invalid native library size: {info.filename}")
            offset = local_data_offset(stream, info)
            require(offset + info.compress_size <= archive.start_dir,
                    f"{info.filename}: native payload overlaps ZIP directory")
            ranges.append((info.header_offset, offset + info.compress_size))
            if kind == "apk":
                require(info.compress_type == zipfile.ZIP_STORED,
                        f"{info.filename}: APK native library must be uncompressed")
                require(offset % PAGE == 0, f"{info.filename}: APK ZIP offset not 16 KB aligned")
            details = check_elf(archive.read(info), abi, info.filename)
            if kind == "apk" or info.filename.startswith("base/"):
                found[abi].add(filename)
            libraries.append({"entry": info.filename, "abi": abi, **details})
        ranges.sort()
        require(all(right[0] >= left[1] for left, right in zip(ranges, ranges[1:])),
                "overlapping native ZIP entries")
        for abi, libs in found.items():
            require(REQUIRED_LIBS <= libs, f"{abi}: missing required libraries {sorted(REQUIRED_LIBS - libs)}")
    return {"artifact": str(path), "kind": kind, "libraries": libraries}


def run_tool(command):
    result = subprocess.run(command, check=True, text=True, capture_output=True, timeout=180)
    return result.stdout


def check_bundle_config(output):
    # bundletool DumpManager prints protobuf JSON, not protobuf text format.
    def unique_pairs(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"duplicate bundle config field: {key}")
            result[key] = value
        return result

    try:
        config = json.loads(output, object_pairs_hook=unique_pairs)
        native = config["optimizations"]["uncompressNativeLibraries"]
        require(native.get("enabled") is True, "AAB config must enable uncompressed native libraries")
        require(native.get("alignment") == "PAGE_ALIGNMENT_16K",
                "AAB config must specify PAGE_ALIGNMENT_16K")
    except (ValueError, KeyError, TypeError, AttributeError) as exc:
        raise InvalidArtifact(f"invalid AAB native packaging config: {exc}") from exc


def check_artifact(path, expected_abis, zipalign=None, bundletool=None, java="java", require_unsigned=False):
    result = inspect_archive(path, expected_abis, require_unsigned)
    if result["kind"] == "apk":
        require(zipalign is not None, "APK validation requires explicit SDK --zipalign")
        run_tool([str(zipalign), "-c", "-P", "16", "-v", "4", str(path)])
        result["zipalign"] = "passed"
        if require_unsigned:
            result["unsigned_structure"] = "no v1 signature entries or APK Signing Block"
    else:
        require(bundletool is not None, "AAB validation requires explicit verified --bundletool jar")
        output = run_tool([str(java), "-jar", str(bundletool), "dump", "config", f"--bundle={path}"])
        check_bundle_config(output)
        result["bundle_alignment"] = "PAGE_ALIGNMENT_16K"
        result["derived_apks"] = "not checked by AAB inspection; validate generated APKs separately"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifacts", type=Path, nargs="+")
    parser.add_argument("--abi", action="append", choices=sorted(MACHINES), dest="abis",
                        help="expected ABI (repeat); default: both shipped ABIs")
    parser.add_argument("--zipalign", type=Path)
    parser.add_argument("--bundletool", type=Path)
    parser.add_argument("--java", default="java")
    parser.add_argument("--require-unsigned", action="store_true",
                        help="reject v1 signature entries and v2/v3 signing blocks; APK inputs only")
    args = parser.parse_args()
    try:
        results = [check_artifact(p, set(args.abis or MACHINES), args.zipalign, args.bundletool, args.java, args.require_unsigned)
                   for p in args.artifacts]
    except (InvalidArtifact, OSError, zipfile.BadZipFile, RuntimeError, subprocess.SubprocessError) as exc:
        print(f"Android artifact check failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps({"artifacts": results, "runtime_16kb": "not established by artifact checks"}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
