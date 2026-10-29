#!/usr/bin/env python3
"""Validate an unsigned device archive's build structure; NOT App Store approval.

Run on macOS after xcodebuild archive. Records imported native API symbols per
executable so privacy review uses actual linked artifacts as well as source.
A successful structural check deliberately leaves privacy_review_complete false.
"""
import argparse
import json
from pathlib import Path
import plistlib
import re
import subprocess


EXPECTED = {
    "PrivacyInfo.xcprivacy": {"NSPrivacyAccessedAPICategoryUserDefaults": ["CA92.1"]},
    "HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy": {
        "NSPrivacyAccessedAPICategoryFileTimestamp": ["C617.1"]},
    "Frameworks/OmcpFFI.framework/PrivacyInfo.xcprivacy": {
        "NSPrivacyAccessedAPICategoryFileTimestamp": ["C617.1"]},
}
# Imported symbols cannot identify Objective-C selectors or all indirect calls.
# This is a reproducible review inventory, not an exhaustive privacy-API detector.
API_SYMBOL = re.compile(
    r"\b_(?:f?stat(?:at|fs|vfs)?|lstat|getattrlist(?:bulk|at)?|fgetattrlist|"
    r"mach_absolute_time|clock_gettime(?:_nsec_np)?)(?:\$\w+)?\b")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_plist(path):
    require(path.is_file() and not path.is_symlink(), f"Missing/unsafe plist: {path}")
    with path.open("rb") as stream:
        data = plistlib.load(stream)
    require(isinstance(data, dict), f"Expected plist dictionary: {path}")
    return data


def validate_manifest(path, expected):
    manifest = read_plist(path)
    require(set(manifest) == {"NSPrivacyAccessedAPITypes"},
            f"Unexpected privacy declarations (review required): {path}")
    entries = manifest["NSPrivacyAccessedAPITypes"]
    require(isinstance(entries, list), f"Invalid API list: {path}")
    actual = {}
    for entry in entries:
        require(isinstance(entry, dict) and set(entry) == {
            "NSPrivacyAccessedAPIType", "NSPrivacyAccessedAPITypeReasons"},
            f"Invalid API entry: {path}")
        category = entry["NSPrivacyAccessedAPIType"]
        require(isinstance(category, str) and category not in actual,
                f"Invalid/duplicate API category: {path}")
        actual[category] = entry["NSPrivacyAccessedAPITypeReasons"]
    require(actual == expected, f"Unreviewed API reasons: {path}: {actual}")
    return actual


def output(command):
    return subprocess.run(command, check=True, capture_output=True, text=True,
                          timeout=120).stdout


MACHO_MAGICS = {
    b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe",  # 32-bit, both byte orders
    b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe",  # 64-bit
    b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",  # universal
    b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca",  # universal 64-bit
}


def is_macho(path):
    with path.open("rb") as stream:
        return stream.read(4) in MACHO_MAGICS


def validate_binary(binary, source_owned=True):
    require(binary.is_file() and not binary.is_symlink(), f"Missing/unsafe executable: {binary}")
    arches = output(["xcrun", "lipo", "-archs", str(binary)]).strip().split()
    require(arches, f"Missing architecture inventory: {binary}")
    if source_owned:
        require(arches == ["arm64"], f"Not a device arm64-only executable: {binary}: {arches}")
    loads = output(["xcrun", "otool", "-l", str(binary)])
    require(loads.strip(), f"Missing Mach-O load-command inventory: {binary}")
    embedded_signature = bool(re.search(r"^\s*cmd LC_CODE_SIGNATURE\s*$", loads, re.M))
    if source_owned:
        require(not embedded_signature,
                f"Archive executable has an embedded code signature: {binary}")
    blocks = re.findall(r"cmd LC_BUILD_VERSION\n(.*?)(?=Load command|\Z)", loads, re.S)
    versions = [dict(re.findall(r"^\s*(platform|minos|sdk)\s+(\S+)", block, re.M))
                for block in blocks]
    if source_owned:
        require(len(versions) == 1, f"Missing/ambiguous LC_BUILD_VERSION: {binary}")
        fields = versions[0]
        require(fields.get("platform") in {"2", "IOS"}, f"Not iOS device platform: {binary}")
        require(fields.get("minos") in {"17.0", "17.0.0"}, f"Changed minimum OS: {binary}")
        require(fields.get("sdk") in {"26.2", "26.2.0"}, f"Wrong Mach-O SDK: {binary}")
    symbols = output(["xcrun", "nm", "-u", str(binary)])
    return {"source_owned": source_owned, "architectures": arches,
            "embedded_code_signature": embedded_signature,
            "build_versions": versions, "load_commands": loads,
            "required_reason_review_symbols": sorted(set(API_SYMBOL.findall(symbols))),
            "undefined_symbols": symbols}


def validate_archive(archive):
    archive = Path(archive).resolve()
    app = archive / "Products/Applications/OpenMeshChat.app"
    require(app.is_dir() and not app.is_symlink(), f"Missing/unsafe app: {app}")
    # iOS bundles need no macOS-style version symlinks. Reject unsurveyed targets
    # rather than following a link outside the archive or silently skipping code.
    require(not any(path.is_symlink() for path in app.rglob("*")),
            "Archive app contains an unsupported symlink")
    # Signing/export/provisioning are explicitly outside this packaging workflow.
    require(not list(app.rglob("embedded.mobileprovision")), "Archive contains a provisioning profile")
    # A bundled vendor framework may retain its vendor signature. Check only
    # source-owned bundle signing here; preserve and inventory all vendor code.
    manifests = {relative: validate_manifest(app / relative, expected)
                 for relative, expected in EXPECTED.items()}
    # SwiftPM's manifest resource must also accompany the package linked by the extension.
    extension = app / "PlugIns/NotificationService.appex"
    require(extension.is_dir(), "Missing notification extension")
    extension_manifest = extension / "HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy"
    manifests[str(extension_manifest.relative_to(app))] = validate_manifest(
        extension_manifest, EXPECTED["HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy"])
    bundles = [(app, "chat.auralis.app"),
               (extension, "chat.auralis.app.notifications"),
               (app / "Frameworks/OmcpFFI.framework", "chat.omcp.ffi")]
    binaries = {}
    for bundle, identifier in bundles:
        require(not (bundle / "_CodeSignature").exists(),
                f"Source-owned archive bundle is unexpectedly signed: {bundle}")
        info = read_plist(bundle / "Info.plist")
        require(info.get("CFBundleIdentifier") == identifier, f"Wrong bundle identity: {bundle}")
        require(info.get("MinimumOSVersion") == "17.0", f"Wrong minimum OS: {bundle}")
        require(info.get("DTSDKName") == "iphoneos26.2", f"Wrong archive SDK: {bundle}")
        require(info.get("DTXcodeBuild") == "17C529", f"Wrong Xcode build: {bundle}")
        name = info.get("CFBundleExecutable")
        require(isinstance(name, str) and name and Path(name).name == name,
                f"Unsafe/missing executable name: {bundle}")
        binary = bundle / name
        binaries[str(binary.relative_to(app))] = validate_binary(binary)
    # Inventory every remaining packaged Mach-O, including Swift runtime dylibs
    # and extra frameworks. Vendor tools may use different deployment/SDK values;
    # preserve those rather than asserting our source-owned build pins on them.
    for path in sorted(app.rglob("*")):
        relative = str(path.relative_to(app))
        if path.is_file() and relative not in binaries and is_macho(path):
            binaries[relative] = validate_binary(path, source_owned=False)
    return {
        "structure_valid": True,
        "privacy_review_complete": False,
        "release_ready": False,
        "manifests": manifests,
        "binaries": binaries,
        "open_review_items": [
            "Bundled SQLite Apple fstatfs/statfs filesystem-type inspection has no verified matching approved DiskSpace reason; do not invent one.",
            "Review linked native symbols and indirect/Foundation API calls; symbol absence alone is not proof of non-use.",
            "Additional/vendor Mach-O signatures are inventoried, not stripped or authenticated; vendor runtime provenance/signature review remains open.",
            "Data collection/tracking and store privacy questionnaire answers are not declared or validated here.",
            "Minimum iOS 17 runtime remains untested; CI covers iOS 18.5 and 26.2.",
            "SecureCapture undocumented canvas behavior is unchanged and requires a separate owner product/security decision.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = validate_archive(args.archive)
    except (ValueError, OSError, plistlib.InvalidFileException,
            subprocess.SubprocessError) as error:
        parser.exit(1, f"iOS archive structural validation failed: {error}\n")
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print("Unsigned archive structure validated; privacy review INCOMPLETE (see report).")


if __name__ == "__main__":
    main()
