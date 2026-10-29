#!/usr/bin/env bash
# Validate actual artifacts; never signs or uploads a package.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
APK=${1:?usage: validate-android-packages.sh APK AAB}
AAB=${2:?usage: validate-android-packages.sh APK AAB}
: "${ANDROID_HOME:?}" "${RUNNER_TEMP:?}"
TOOLS="$RUNNER_TEMP/auralis-bundletool"
mkdir -p "$TOOLS"
curl --fail --location --retry 3 --max-time 180 \
  https://github.com/google/bundletool/releases/download/1.18.2/bundletool-all-1.18.2.jar \
  -o "$TOOLS/bundletool.jar"
echo "378b5434cd1378bef6b2bc527b8c7f0ff2584b273830335bce54d6d0813c8584  $TOOLS/bundletool.jar" | sha256sum --check --strict
ZIPALIGN="$ANDROID_HOME/build-tools/35.0.0/zipalign"
python3 "$ROOT/scripts/ci/check-android-artifacts.py" "$APK" "$AAB" \
  --zipalign "$ZIPALIGN" --bundletool "$TOOLS/bundletool.jar"
# bundletool documents unsigned output when no debug keystore exists. Isolate
# every Android/home preference location: never select an existing signing key.
UNSIGNED_HOME=$(mktemp -d "$TOOLS/unsigned-home.XXXXXX")
HOME="$UNSIGNED_HOME" ANDROID_USER_HOME="$UNSIGNED_HOME" \
  ANDROID_PREFS_ROOT="$UNSIGNED_HOME" ANDROID_SDK_HOME="$UNSIGNED_HOME" \
  java -Duser.home="$UNSIGNED_HOME" -jar "$TOOLS/bundletool.jar" build-apks \
  --bundle="$AAB" --output="$TOOLS/derived.apks" --mode=universal --overwrite
unzip -p "$TOOLS/derived.apks" universal.apk > "$TOOLS/derived-unsigned.apk"
# Prove absence of v1/v2/v3 signing material, not merely a failed verifier
# invocation (which could also mean a missing tool or malformed signature).
python3 "$ROOT/scripts/ci/check-android-artifacts.py" "$TOOLS/derived-unsigned.apk" \
  --zipalign "$ZIPALIGN" --require-unsigned
echo 'ELF, APK ZIP, AAB config and unsigned bundle-derived universal APK checks passed.'
