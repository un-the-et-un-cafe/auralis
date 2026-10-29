"""Archive guard fixtures; never invoke Xcode, codesign, or a device."""
import importlib.util
from pathlib import Path
import plistlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "ios_archive", Path(__file__).parents[1] / "ci/check-ios-archive.py")
ios = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ios)


class IOSArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.archive = Path(self.temp.name) / "Auralis.xcarchive"
        self.app = self.archive / "Products/Applications/OpenMeshChat.app"
        for relative, expected in ios.EXPECTED.items():
            self.manifest(self.app / relative, expected)
        extension = self.app / "PlugIns/NotificationService.appex"
        self.manifest(extension / "HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy",
                      ios.EXPECTED["HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy"])
        for bundle, name, identifier in [
            (self.app, "OpenMeshChat", "chat.auralis.app"),
            (extension, "NotificationService", "chat.auralis.app.notifications"),
            (self.app / "Frameworks/OmcpFFI.framework", "OmcpFFI", "chat.omcp.ffi"),
        ]:
            self.plist(bundle / "Info.plist", {
                "CFBundleExecutable": name, "CFBundleIdentifier": identifier, "MinimumOSVersion": "17.0",
                "DTSDKName": "iphoneos26.2", "DTXcodeBuild": "17C529"})
            (bundle / name).write_bytes(b"fixture-not-executable")
        mock = patch.object(ios, "output", side_effect=self.command)
        self.commands = mock.start()
        self.addCleanup(mock.stop)

    def plist(self, path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(plistlib.dumps(data))

    def manifest(self, path, reasons):
        self.plist(path, {"NSPrivacyAccessedAPITypes": [
            {"NSPrivacyAccessedAPIType": k, "NSPrivacyAccessedAPITypeReasons": v}
            for k, v in reasons.items()]})

    def command(self, command):
        if command[1] == "lipo":
            return "arm64\n"
        if command[1] == "otool":
            return "Load command 1\n      cmd LC_BUILD_VERSION\n  cmdsize 32\n platform 2\n    minos 17.0\n      sdk 26.2\n   ntools 1\n"
        if command[1] == "nm":
            return "_fstat$INODE64\n_fstatfs\n_clock_gettime\n_random\n"
        raise AssertionError(command)

    def test_repository_manifests_match_guard_without_data_declarations(self):
        root = Path(__file__).parents[2]
        for path, expected in [
            ("apps/ios/OpenMeshChat/PrivacyInfo.xcprivacy", ios.EXPECTED["PrivacyInfo.xcprivacy"]),
            ("apps/ios/HoplaKit/Sources/HoplaKit/PrivacyInfo.xcprivacy",
             ios.EXPECTED["HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy"]),
            ("apps/ios/Privacy/OmcpFFI/PrivacyInfo.xcprivacy",
             ios.EXPECTED["Frameworks/OmcpFFI.framework/PrivacyInfo.xcprivacy"]),
        ]:
            with self.subTest(path=path):
                ios.validate_manifest(root / path, expected)

    def test_valid_structure_never_claims_privacy_or_release_complete(self):
        result = ios.validate_archive(self.archive)
        self.assertTrue(result["structure_valid"])
        self.assertFalse(result["privacy_review_complete"])
        self.assertFalse(result["release_ready"])
        self.assertEqual(len(result["binaries"]), 3)
        symbols = result["binaries"]["OpenMeshChat"]["required_reason_review_symbols"]
        self.assertEqual(symbols, ["_clock_gettime", "_fstat$INODE64", "_fstatfs"])
        self.assertEqual(self.commands.call_count, 9)

    def test_missing_manifest_fails(self):
        (self.app / "PrivacyInfo.xcprivacy").unlink()
        with self.assertRaisesRegex(ValueError, "Missing/unsafe plist"):
            ios.validate_archive(self.archive)

    def test_missing_extension_package_resource_fails(self):
        (self.app / "PlugIns/NotificationService.appex/HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy").unlink()
        with self.assertRaisesRegex(ValueError, "Missing/unsafe plist"):
            ios.validate_archive(self.archive)

    def test_no_unreviewed_collection_or_tracking_declarations(self):
        path = self.app / "PrivacyInfo.xcprivacy"
        data = plistlib.loads(path.read_bytes())
        data["NSPrivacyTracking"] = False
        self.plist(path, data)
        with self.assertRaisesRegex(ValueError, "Unexpected privacy declarations"):
            ios.validate_archive(self.archive)

    def test_wrong_reason_fails(self):
        self.manifest(self.app / "PrivacyInfo.xcprivacy",
                      {"NSPrivacyAccessedAPICategoryUserDefaults": ["1C8F.1"]})
        with self.assertRaisesRegex(ValueError, "Unreviewed API reasons"):
            ios.validate_archive(self.archive)

    def test_duplicate_category_fails(self):
        path = self.app / "PrivacyInfo.xcprivacy"
        data = plistlib.loads(path.read_bytes())
        data["NSPrivacyAccessedAPITypes"] *= 2
        self.plist(path, data)
        with self.assertRaisesRegex(ValueError, "duplicate API"):
            ios.validate_archive(self.archive)

    def test_signature_and_profile_fail(self):
        for name in ["_CodeSignature", "embedded.mobileprovision"]:
            path = self.app / name
            path.write_bytes(b"fixture")
            with self.assertRaises(ValueError):
                ios.validate_archive(self.archive)
            path.unlink()

    def test_wrong_build_metadata_fails(self):
        path = self.app / "Info.plist"
        original = plistlib.loads(path.read_bytes())
        for key, value in [("CFBundleIdentifier", "unrelated.app"), ("MinimumOSVersion", "18.0"), ("DTSDKName", "iphoneos18.5"),
                           ("DTXcodeBuild", "16F6"), ("CFBundleExecutable", "../outside")]:
            with self.subTest(key=key):
                self.plist(path, {**original, key: value})
                with self.assertRaises(ValueError):
                    ios.validate_archive(self.archive)
        self.plist(path, original)

    def test_simulator_or_old_sdk_binary_fails_even_with_correct_plist(self):
        for bad in ["platform 7", "sdk 18.5", "minos 18.0"]:
            def command(args):
                result = self.command(args)
                if args[1] == "otool":
                    key = bad.split()[0]
                    old = {"platform": "platform 2", "sdk": "sdk 26.2", "minos": "minos 17.0"}[key]
                    return result.replace(old, bad)
                return result
            self.commands.side_effect = command
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                ios.validate_archive(self.archive)

    def test_missing_macho_metadata_or_architecture_fails(self):
        for mode in ["lipo", "otool"]:
            self.commands.side_effect = lambda args: "" if args[1] == mode else self.command(args)
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                ios.validate_archive(self.archive)

    def test_embedded_adhoc_signature_fails_without_signature_directory(self):
        def command(args):
            result = self.command(args)
            if args[1] == "otool":
                return result + "Load command 2\n      cmd LC_CODE_SIGNATURE\n  cmdsize 16\n"
            return result
        self.commands.side_effect = command
        self.assertFalse((self.app / "_CodeSignature").exists())
        with self.assertRaisesRegex(ValueError, "embedded code signature"):
            ios.validate_archive(self.archive)

    def test_additional_macho_is_inventoried_without_own_sdk_pins(self):
        extra = self.app / "Frameworks/libswiftExample.dylib"
        extra.write_bytes(b"\xcf\xfa\xed\xfe" + b"mock")
        (self.app / "normal-resource.dat").write_bytes(b"not native code")
        def command(args):
            result = self.command(args)
            if args[-1] == str(extra) and args[1] == "otool":
                return result.replace("minos 17.0", "minos 15.0").replace("sdk 26.2", "sdk 25.0")
            return result
        self.commands.side_effect = command
        result = ios.validate_archive(self.archive)
        record = result["binaries"]["Frameworks/libswiftExample.dylib"]
        self.assertFalse(record["source_owned"])
        self.assertEqual(record["build_versions"][0]["minos"], "15.0")
        self.assertEqual(record["build_versions"][0]["sdk"], "25.0")
        self.assertIn("_fstatfs", record["required_reason_review_symbols"])
        self.assertEqual(len(result["binaries"]), 4)
        self.assertFalse(result["privacy_review_complete"])
        self.assertNotIn("NSPrivacyAccessedAPICategoryDiskSpace", str(result["manifests"]))

    def test_vendor_signature_is_preserved_and_reported_without_authentication(self):
        extra = self.app / "Frameworks/libswiftExample.dylib"
        extra.write_bytes(b"\xcf\xfa\xed\xfe" + b"mock")
        def command(args):
            result = self.command(args)
            if args[-1] == str(extra) and args[1] == "otool":
                return result + "Load command 2\n      cmd LC_CODE_SIGNATURE\n"
            return result
        self.commands.side_effect = command
        result = ios.validate_archive(self.archive)
        record = result["binaries"]["Frameworks/libswiftExample.dylib"]
        self.assertTrue(record["embedded_code_signature"])
        self.assertFalse(record["source_owned"])
        self.assertFalse(result["binaries"]["OpenMeshChat"]["embedded_code_signature"])
        self.assertIn("provenance/signature review", " ".join(result["open_review_items"]))

    def test_vendor_framework_signature_directory_is_not_stripped_or_rejected(self):
        signature = self.app / "Frameworks/Vendor.framework/_CodeSignature/CodeResources"
        signature.parent.mkdir(parents=True)
        signature.write_bytes(b"vendor-signed-resource")
        ios.validate_archive(self.archive)
        self.assertEqual(signature.read_bytes(), b"vendor-signed-resource")

    def test_macho_magic_formats_and_plain_resources(self):
        path = self.app / "probe"
        for magic in ios.MACHO_MAGICS:
            path.write_bytes(magic + b"fixture")
            self.assertTrue(ios.is_macho(path))
        for data in [b"", b"abc", b"ordinary image"]:
            path.write_bytes(data)
            self.assertFalse(ios.is_macho(path))

    def test_native_tool_failure_is_not_swallowed(self):
        self.commands.side_effect = subprocess.CalledProcessError(1, ["xcrun", "nm"])
        with self.assertRaises(subprocess.CalledProcessError):
            ios.validate_archive(self.archive)

    def test_symlink_manifest_rejected(self):
        path = self.app / "PrivacyInfo.xcprivacy"
        copy = self.app / "copy.plist"
        path.rename(copy)
        path.symlink_to(copy)
        with self.assertRaisesRegex(ValueError, "unsupported symlink"):
            ios.validate_archive(self.archive)


if __name__ == "__main__":
    unittest.main()
