#!/usr/bin/env python3
"""Apply Ghost Talk's idempotent mobile-native privacy/permission configuration."""
from __future__ import annotations

import argparse
import plistlib
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ANDROID_NS = "http://schemas.android.com/apk/res/android"
ANDROID_PERMISSIONS = (
    "android.permission.INTERNET",
    "android.permission.ACCESS_NETWORK_STATE",
    "android.permission.ACCESS_WIFI_STATE",
    "android.permission.CHANGE_WIFI_MULTICAST_STATE",
    "android.permission.RECORD_AUDIO",
    "android.permission.CAMERA",
)
IOS_PRIVACY = {
    "NSMicrophoneUsageDescription": "Ghost Talk uses the microphone for voice messages and calls you start.",
    "NSCameraUsageDescription": "Ghost Talk uses the camera when you choose to capture media.",
    "NSPhotoLibraryUsageDescription": "Ghost Talk accesses photos only when you choose media to share or use as a backup carrier.",
    "NSLocalNetworkUsageDescription": "Ghost Talk uses the local network for peer discovery and direct communication you enable.",
}


def fail(message: str) -> None:
    raise RuntimeError(message)


def find_one(root: Path, name: str) -> Path:
    candidates = [path for path in root.rglob(name) if "build" not in path.parts]
    if not candidates:
        fail(f"could not find {name} below {root}")
    candidates.sort(key=lambda path: (len(path.parts), str(path)))
    return candidates[0]


def configure_android(native_root: Path) -> Path:
    manifest = find_one(native_root / "gen" / "android", "AndroidManifest.xml")
    ET.register_namespace("android", ANDROID_NS)
    tree = ET.parse(manifest)
    root = tree.getroot()
    name_key = f"{{{ANDROID_NS}}}name"
    current = {node.get(name_key) for node in root.findall("uses-permission")}
    insert_at = next((i for i, node in enumerate(list(root)) if node.tag == "application"), len(root))
    for permission in reversed(ANDROID_PERMISSIONS):
        if permission not in current:
            root.insert(insert_at, ET.Element("uses-permission", {name_key: permission}))
    feature_name = f"{{{ANDROID_NS}}}name"
    feature_required = f"{{{ANDROID_NS}}}required"
    features = {node.get(feature_name) for node in root.findall("uses-feature")}
    for feature in ("android.hardware.microphone", "android.hardware.camera.any"):
        if feature not in features:
            root.insert(insert_at, ET.Element("uses-feature", {feature_name: feature, feature_required: "false"}))
    app = root.find("application")
    if app is not None:
        app.set(f"{{{ANDROID_NS}}}usesCleartextTraffic", "false")
    tree.write(manifest, encoding="utf-8", xml_declaration=True)
    return manifest


def configure_ios(native_root: Path) -> Path:
    plist = find_one(native_root / "gen" / "apple", "Info.plist")
    with plist.open("rb") as handle:
        data = plistlib.load(handle)
    data.update(IOS_PRIVACY)
    data["UISupportedInterfaceOrientations"] = [
        "UIInterfaceOrientationPortrait",
        "UIInterfaceOrientationLandscapeLeft",
        "UIInterfaceOrientationLandscapeRight",
    ]
    data["UISupportedInterfaceOrientations~ipad"] = [
        "UIInterfaceOrientationPortrait",
        "UIInterfaceOrientationPortraitUpsideDown",
        "UIInterfaceOrientationLandscapeLeft",
        "UIInterfaceOrientationLandscapeRight",
    ]
    with plist.open("wb") as handle:
        plistlib.dump(data, handle, sort_keys=False)
    return plist


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("platform", choices=("android", "ios"))
    parser.add_argument("native_root", type=Path)
    args = parser.parse_args()
    native_root = args.native_root.resolve()
    if not native_root.is_dir():
        fail(f"native root does not exist: {native_root}")
    configured = configure_android(native_root) if args.platform == "android" else configure_ios(native_root)
    print(f"Configured Ghost Talk {args.platform} native project: {configured}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
