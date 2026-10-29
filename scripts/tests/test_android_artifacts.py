"""Synthetic malformed/native artifact fixtures; no toolchains or app execution."""

import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "android_artifacts", Path(__file__).parents[1] / "ci" / "check-android-artifacts.py")
CONFIG = json.dumps({"optimizations": {"uncompressNativeLibraries": {"enabled": True, "alignment": "PAGE_ALIGNMENT_16K"}}})
android = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(android)


def elf(abi="arm64-v8a", alignment=16384, vaddr=0, relro=None):
    count = 1 + (relro is not None)
    size = 64 + 56 * count
    result = bytearray(b"\x7fELF\x02\x01\x01" + b"\0" * 9)
    result.extend(struct.pack("<HHIQQQIHHHHHH", 3, android.MACHINES[abi], 1,
                              0, 64, 0, 0, 64, 56, count, 0, 0, 0))
    result.extend(struct.pack("<IIQQQQQQ", 1, 5, 0, vaddr, 0, size, size, alignment))
    if relro is not None:
        result.extend(struct.pack("<IIQQQQQQ", 0x6474E552, 4, 0, relro, 0, 0, 0, 1))
    return result


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def archive(self, kind="apk", *, align=True, compression=zipfile.ZIP_STORED,
                omit=None, transform=None, additional=()):
        path = self.root / ("app." + kind)
        with zipfile.ZipFile(path, "w") as output:
            for abi in android.MACHINES:
                for library in sorted(android.REQUIRED_LIBS):
                    name = f"lib/{abi}/{library}"
                    if kind == "aab":
                        name = "base/" + name
                    if omit and omit in name:
                        continue
                    payload = elf(abi)
                    if transform:
                        payload = transform(name, payload)
                    info = zipfile.ZipInfo(name)
                    info.compress_type = compression
                    if align and kind == "apk":
                        offset = output.fp.tell() + 30 + len(name.encode())
                        padding = (-offset) % android.PAGE
                        if padding < 4:
                            padding += android.PAGE
                        info.extra = struct.pack("<HH", 0xFFFF, padding - 4) + b"\0" * (padding - 4)
                    output.writestr(info, payload)
            for name, payload in additional:
                output.writestr(name, payload)
        return path

    def rejects(self, path, message):
        with self.assertRaisesRegex(android.InvalidArtifact, message):
            android.inspect_archive(path, set(android.MACHINES))

    def test_both_abis_and_required_libraries_pass(self):
        result = android.inspect_archive(self.archive(), set(android.MACHINES))
        self.assertEqual(len(result["libraries"]), 4)

    def test_aab_does_not_require_zip_alignment(self):
        result = android.inspect_archive(self.archive("aab", align=False, compression=zipfile.ZIP_DEFLATED),
                                         set(android.MACHINES))
        self.assertEqual(result["kind"], "aab")

    def test_apk_rejects_unaligned_zip_offset(self):
        self.rejects(self.archive(align=False), "ZIP offset")

    def test_apk_rejects_compressed_libraries(self):
        self.rejects(self.archive(compression=zipfile.ZIP_DEFLATED), "uncompressed")

    def test_missing_jna_fails(self):
        self.rejects(self.archive(omit="libjnidispatch"), "missing required")

    def test_missing_abi_fails(self):
        self.rejects(self.archive(omit="x86_64"), "missing required")

    def test_unexpected_native_location_fails(self):
        self.rejects(self.archive(additional=[("assets/hidden.so", bytes(elf()))]), "unexpected native")

    def test_unexpected_abi_fails(self):
        self.rejects(self.archive(additional=[("lib/armeabi-v7a/libextra.so", bytes(elf()))]), "unexpected ABI")

    def test_path_traversal_fails(self):
        self.rejects(self.archive(additional=[("../escape", b"x")]), "unsafe ZIP path")

    def test_duplicate_entry_fails(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            path = self.archive(additional=[("lib/arm64-v8a/libomcp_ffi.so", bytes(elf()))])
        self.rejects(path, "duplicate ZIP entry")

    def test_symlink_entry_fails(self):
        path = self.archive()
        with zipfile.ZipFile(path, "a") as output:
            info = zipfile.ZipInfo("link")
            info.create_system = 3
            info.external_attr = 0o120777 << 16
            output.writestr(info, "elsewhere")
        self.rejects(path, "symlink")

    def test_inconsistent_local_name_fails(self):
        path = self.archive()
        with path.open("r+b") as stream:
            stream.seek(30)
            stream.write(b"bad")
        self.rejects(path, "inconsistent local ZIP name")

    def test_invalid_elf_in_any_dependency_fails(self):
        self.rejects(self.archive(transform=lambda name, data: b"not ELF" if "jnidispatch" in name else data),
                     "truncated ELF")

    def test_machine_mismatch_fails(self):
        with self.assertRaisesRegex(android.InvalidArtifact, "machine mismatch"):
            android.check_elf(elf("x86_64"), "arm64-v8a", "lib")

    def test_4kb_load_fails(self):
        with self.assertRaisesRegex(android.InvalidArtifact, "alignment"):
            android.check_elf(elf(alignment=4096), "arm64-v8a", "lib")

    def test_non_power_of_two_load_fails(self):
        with self.assertRaisesRegex(android.InvalidArtifact, "power of two"):
            android.check_elf(elf(alignment=24576), "arm64-v8a", "lib")

    def test_incongruent_load_fails(self):
        with self.assertRaisesRegex(android.InvalidArtifact, "incongruent"):
            android.check_elf(elf(vaddr=4096), "arm64-v8a", "lib")

    def test_relro_end_alignment(self):
        android.check_elf(elf(relro=16384), "arm64-v8a", "lib")
        with self.assertRaisesRegex(android.InvalidArtifact, "GNU_RELRO"):
            android.check_elf(elf(relro=4096), "arm64-v8a", "lib")

    def test_program_headers_outside_file(self):
        data = elf()
        struct.pack_into("<Q", data, 32, 10000)
        with self.assertRaisesRegex(android.InvalidArtifact, "headers outside"):
            android.check_elf(data, "arm64-v8a", "lib")

    def test_load_outside_file(self):
        data = elf()
        struct.pack_into("<Q", data, 64 + 32, len(data) + 1)
        with self.assertRaisesRegex(android.InvalidArtifact, "outside file"):
            android.check_elf(data, "arm64-v8a", "lib")

    def test_no_load_segments(self):
        data = elf()
        struct.pack_into("<I", data, 64, 0)
        with self.assertRaisesRegex(android.InvalidArtifact, "no PT_LOAD"):
            android.check_elf(data, "arm64-v8a", "lib")

    def test_load_memory_size_too_small(self):
        data = elf()
        struct.pack_into("<Q", data, 64 + 40, 1)
        with self.assertRaisesRegex(android.InvalidArtifact, "filesz exceeds memsz"):
            android.check_elf(data, "arm64-v8a", "lib")

    def test_elf_header_sizes_and_class(self):
        for position, fmt, value in ((4, "<B", 1), (52, "<H", 0), (54, "<H", 1), (56, "<H", 65535)):
            with self.subTest(position=position):
                data = elf()
                struct.pack_into(fmt, data, position, value)
                with self.assertRaises(android.InvalidArtifact):
                    android.check_elf(data, "arm64-v8a", "lib")

    def test_non_native_entry_cannot_hide_unsafe_path(self):
        for name in ("/absolute", "a/./file", "a//file", "a\\file", "a/../file"):
            with self.subTest(name=name):
                self.rejects(self.archive(additional=[(name, b"x")]), "unsafe ZIP path")

    def test_single_abi_derived_apk(self):
        result = android.inspect_archive(self.archive(omit="x86_64"), {"arm64-v8a"})
        self.assertEqual(len(result["libraries"]), 2)

    def test_aab_base_module_cannot_be_replaced_by_feature(self):
        path = self.archive("aab", omit="libomcp_ffi")
        with zipfile.ZipFile(path, "a") as output:
            for abi in android.MACHINES:
                output.writestr(f"feature/lib/{abi}/libomcp_ffi.so", elf(abi))
        self.rejects(path, "missing required")

    def test_unsigned_apk_structure_passes(self):
        android.inspect_archive(self.archive(), set(android.MACHINES), require_unsigned=True)

    def test_unsigned_flag_rejects_aab(self):
        with self.assertRaisesRegex(android.InvalidArtifact, "only to APK"):
            android.inspect_archive(self.archive("aab"), set(android.MACHINES), require_unsigned=True)

    def test_unsigned_rejects_all_v1_signature_entry_types_case_insensitively(self):
        for name in ("META-INF/CERT.SF", "meta-inf/CERT.rsa", "META-INF/CERT.DSA", "META-INF/CERT.EC"):
            with self.subTest(name=name):
                path = self.archive(additional=[(name, b"signature")])
                with self.assertRaisesRegex(android.InvalidArtifact, "v1 signature"):
                    android.inspect_archive(path, set(android.MACHINES), require_unsigned=True)

    def signing_block(self, path, scheme_id):
        data = path.read_bytes()
        with zipfile.ZipFile(path) as archive:
            start = archive.start_dir
        # Deliberately not a valid cryptographic signature: structural rejection
        # must not depend on apksigner succeeding or returning a particular error.
        pair = struct.pack("<QI", 4 + 4, scheme_id) + b"test"
        size = 24 + len(pair)
        block = struct.pack("<Q", size) + pair + struct.pack("<Q", size) + b"APK Sig Block 42"
        result = bytearray(data[:start] + block + data[start:])
        eocd = result.rfind(b"PK\x05\x06")
        struct.pack_into("<I", result, eocd + 16, start + len(block))
        path.write_bytes(result)

    def test_unsigned_rejects_v2_and_v3_blocks(self):
        for scheme in (0x7109871a, 0xf05368c0, 0x1b93ad61):
            with self.subTest(scheme=scheme):
                path = self.archive()
                self.signing_block(path, scheme)
                with self.assertRaisesRegex(android.InvalidArtifact, "APK Signing Block"):
                    android.inspect_archive(path, set(android.MACHINES), require_unsigned=True)

    def test_unsigned_rejects_signing_block_after_prepended_zip_stub(self):
        path = self.archive()
        self.signing_block(path, 0x7109871a)
        path.write_bytes(b"stub" + path.read_bytes())
        with self.assertRaisesRegex(android.InvalidArtifact, "APK Signing Block"):
            android.inspect_archive(path, set(android.MACHINES), require_unsigned=True)

    def test_unsigned_ordinary_meta_inf_entries_allowed(self):
        path = self.archive(additional=[("META-INF/MANIFEST.MF", b"manifest"), ("META-INF/NOTICE", b"notice")])
        android.inspect_archive(path, set(android.MACHINES), require_unsigned=True)

    def test_bundle_config_requires_exact_single_field(self):
        android.check_bundle_config(CONFIG)
        for bad in ("PAGE_ALIGNMENT_16K", "", "{}", "[]", "null",
                    CONFIG.replace("16K", "4K"), CONFIG.replace("true", "false"),
                    CONFIG.replace('"alignment":', '"alignment": "PAGE_ALIGNMENT_4K", "alignment":')):
            with self.subTest(bad=bad), self.assertRaises(android.InvalidArtifact):
                android.check_bundle_config(bad)

    def test_sdk_zipalign_invocation_and_required_tool(self):
        path = self.archive()
        with self.assertRaisesRegex(android.InvalidArtifact, "--zipalign"):
            android.check_artifact(path, set(android.MACHINES))
        with patch.object(android, "run_tool", return_value="Verification successful") as tool:
            android.check_artifact(path, set(android.MACHINES), zipalign="/sdk/zipalign")
            tool.assert_called_once_with(["/sdk/zipalign", "-c", "-P", "16", "-v", "4", str(path)])

    def test_bundletool_invocation_and_required_tool(self):
        path = self.archive("aab")
        with self.assertRaisesRegex(android.InvalidArtifact, "--bundletool"):
            android.check_artifact(path, set(android.MACHINES))
        with patch.object(android, "run_tool", return_value=CONFIG) as tool:
            result = android.check_artifact(path, set(android.MACHINES), bundletool="/verified/tool.jar")
            tool.assert_called_once_with(["java", "-jar", "/verified/tool.jar", "dump", "config", f"--bundle={path}"])
            self.assertIn("not checked", result["derived_apks"])

    def test_tool_failure_is_not_a_pass(self):
        with patch.object(android, "run_tool", side_effect=subprocess.CalledProcessError(1, ["zipalign"])):
            with self.assertRaises(subprocess.CalledProcessError):
                android.check_artifact(self.archive(), set(android.MACHINES), zipalign="/sdk/zipalign")


if __name__ == "__main__":
    unittest.main()
