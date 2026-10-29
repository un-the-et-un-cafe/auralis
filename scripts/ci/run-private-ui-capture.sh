#!/usr/bin/env bash
# Private UI capture on a disposable GitHub-hosted emulator only.
# Uses the isolated .uicapture package; capture privilege exists only in instrumentation.
set -euo pipefail
STAGE=preflight
trap 'result=$?; echo "Android emulator failed during $STAGE at line $LINENO (exit $result)." >&2' ERR

API=${1:?usage: run-android-emulator-tests.sh 31|35}
case "$API" in
  31) IMAGE_REVISION=5; IMAGE_SHA256=f77492768c6449a20d101a2fc0f32865a0aca6067dae6de4e94073fbceda9bca ;;
  35) IMAGE_REVISION=2; IMAGE_SHA256=6dd7de33e63ef105cf2fabea6badda1dbe7665c96d8908e5f6e1407e63ff4556 ;;
  *) echo 'Only reviewed API 31 and 35 images are supported.' >&2; exit 2 ;;
esac
: "${ANDROID_HOME:?}" "${ANDROID_AVDMANAGER:?}" "${RUNNER_TEMP:?}"
REPORTS="$PWD/app/build/reports/emulator"
mkdir -p "$REPORTS"
# This is an access check, not a permission repair. A missing/inaccessible KVM
# device is a real infrastructure blocker, not permission to use software TCG.
{
  uname -a
  id
  if [[ -e /dev/kvm ]]; then stat /dev/kvm; fi
} > "$REPORTS/host.txt"
if [[ $(uname -m) != x86_64 || ! -c /dev/kvm || -L /dev/kvm || ! -r /dev/kvm || ! -w /dev/kvm ]]; then
  cat "$REPORTS/host.txt"
  echo 'KVM must already be a readable/writable character device for this runner. No permissions changed.' >&2
  exit 1
fi
if [[ ${GITHUB_ACTIONS:-} != true || ${RUNNER_ENVIRONMENT:-} != github-hosted ]]; then
  echo 'This installer is restricted to ephemeral GitHub-hosted runners.' >&2
  exit 1
fi

TOOLS="$RUNNER_TEMP/auralis-emulator-tools"
mkdir -p "$TOOLS"
curl --fail --location --retry 3 --max-time 300 \
  https://dl.google.com/android/repository/emulator-linux_x64-16428233.zip -o "$TOOLS/emulator.zip"
echo "b08fc43d8608d2955f607f1b287beb525041e730086bdca3af152accac3af9c1  $TOOLS/emulator.zip" | sha256sum --check --strict
unzip -q "$TOOLS/emulator.zip" -d "$TOOLS"
# avdmanager checks the canonical emulator package and invokes its mksdcard.
# Preserve any runner-provided version, then install only our verified archive.
if [[ -e "$ANDROID_HOME/emulator" || -L "$ANDROID_HOME/emulator" ]]; then
  mv "$ANDROID_HOME/emulator" "$TOOLS/runner-emulator"
fi
mv "$TOOLS/emulator" "$ANDROID_HOME/emulator"
# Official archive lacks SDK Manager registration; avdmanager requires it even
# though the emulator binary can run standalone. Register only the checked version.
grep -Fx 'Pkg.Revision=37.2.12' "$ANDROID_HOME/emulator/source.properties"
cp "$(dirname "$0")/android-emulator-package.xml" "$ANDROID_HOME/emulator/package.xml"
EMULATOR="$ANDROID_HOME/emulator/emulator"
# Use the headless executable for preflight too: the GUI binary links host
# PulseAudio/Qt libraries that a minimal hosted runner may lack.
"$EMULATOR" -no-window -version | tee "$REPORTS/emulator-version.txt"
"$EMULATOR" -no-window -accel-check | tee "$REPORTS/acceleration.txt"

# Exact official archives, not sdkmanager's changing latest image revision.
IMAGE_FILE="x86_64-${API}_r$(printf '%02d' "$IMAGE_REVISION").zip"
curl --fail --location --retry 3 --max-time 300 \
  "https://dl.google.com/android/repository/sys-img/android/$IMAGE_FILE" -o "$TOOLS/image.zip"
echo "$IMAGE_SHA256  $TOOLS/image.zip" | sha256sum --check --strict
IMAGE_DIR="$ANDROID_HOME/system-images/android-$API/default"
mkdir -p "$IMAGE_DIR"
if [[ -e "$IMAGE_DIR/x86_64" || -L "$IMAGE_DIR/x86_64" ]]; then
  mv "$IMAGE_DIR/x86_64" "$TOOLS/runner-system-image"
fi
unzip -q "$TOOLS/image.zip" -d "$IMAGE_DIR"
cp "$IMAGE_DIR/x86_64/source.properties" "$REPORTS/system-image.txt"
export ANDROID_AVD_HOME="$RUNNER_TEMP/auralis-avd-$API"
export ANDROID_USER_HOME="$RUNNER_TEMP/auralis-android-user-$API"
mkdir -p "$ANDROID_AVD_HOME" "$ANDROID_USER_HOME"
printf 'no\n' | "$ANDROID_AVDMANAGER" create avd --force --name auralis-ci \
  --package "system-images;android-$API;default;x86_64" --device pixel_2
test -s "$ANDROID_AVD_HOME/auralis-ci.avd/config.ini"
test -s "$ANDROID_AVD_HOME/auralis-ci.ini"
ADB="$ANDROID_HOME/platform-tools/adb"
export ANDROID_SERIAL=emulator-5554
EMULATOR_PID=
LOGCAT_PID=
source "$(dirname "$0")/android-emulator-cleanup.sh"
cleanup() {
  local result=$?
  trap - EXIT
  if [[ $result != 0 && -n ${theme:-} ]]; then
    # Retain only completed, test-owned captures privately for diagnosis/review.
    mkdir -p "app/build/private-ui-captures/$theme" "app/build/private-ui-captures/_diagnostics"
    "$ADB" -s "$ANDROID_SERIAL" pull /sdcard/Android/data/chat.auralis.app.uicapture/files/ui-captures/. \
      "app/build/private-ui-captures/$theme/" || true
    "$ADB" -s "$ANDROID_SERIAL" logcat -d -v threadtime 'AndroidRuntime:E' '*:S' \
      > app/build/private-ui-captures/_diagnostics/android-runtime-errors.txt || true
  fi
  if ! auralis_cleanup_android "$ADB" "$ANDROID_SERIAL" "$EMULATOR_PID" "$LOGCAT_PID"; then
    echo "Android cleanup incomplete; preserving original test exit status $result." >&2
  fi
  exit "$result"
}
trap cleanup EXIT
STAGE=adb-server
# Start the transport before QEMU attempts its own post-boot ADB configuration.
"$ADB" start-server
STAGE=emulator-launch
"$EMULATOR" -avd auralis-ci -port 5554 -accel on -gpu swiftshader \
  -no-window -no-audio -no-boot-anim -no-snapshot -wipe-data \
  -cores 2 -memory 2048 > "$REPORTS/emulator.log" 2>&1 &
EMULATOR_PID=$!
STAGE=android-readiness
# A boot property can precede a transient ADB restart. Require stable transport
# plus package/input/window services; never retry or skip failed app tests.
python3 "$(dirname "$0")/wait_for_android.py" "$ADB" "$ANDROID_SERIAL" "$EMULATOR_PID" \
  2>&1 | tee "$REPORTS/readiness.txt"
"$ADB" -s "$ANDROID_SERIAL" logcat -v threadtime > "$REPORTS/logcat.txt" 2>&1 &
LOGCAT_PID=$!
STAGE=wake-screen
"$ADB" -s "$ANDROID_SERIAL" shell input keyevent KEYCODE_WAKEUP
"$ADB" -s "$ANDROID_SERIAL" shell input keyevent KEYCODE_MENU
# Normal font size for private appearance review; not accessibility-test evidence.
"$ADB" -s "$ANDROID_SERIAL" shell settings put system font_scale 1.0
STAGE=private-ui-capture
INSTALLER="$(dirname "$0")/install-capture-apk.py"
AAPT="$ANDROID_HOME/build-tools/35.0.0/aapt"
python3 "$INSTALLER" "$ADB" "$AAPT" "$ANDROID_SERIAL" "$EMULATOR_PID" \
  chat.auralis.app.uicapture app/build/outputs/apk/uiCapture/app-uiCapture.apk
python3 "$INSTALLER" "$ADB" "$AAPT" "$ANDROID_SERIAL" "$EMULATOR_PID" \
  chat.auralis.app.uicapture.test app/build/outputs/apk/androidTest/uiCapture/app-uiCapture-androidTest.apk
for theme in light dark; do
  if [[ "$theme" == dark ]]; then night=yes; else night=no; fi
  "$ADB" -s "$ANDROID_SERIAL" shell cmd uimode night "$night"
  # Separate package, reset only our disposable synthetic capture data between themes.
  if "$ADB" -s "$ANDROID_SERIAL" shell pm path chat.auralis.app.uicapture | grep -q package:; then
    "$ADB" -s "$ANDROID_SERIAL" shell pm clear chat.auralis.app.uicapture
  fi
  "$ADB" -s "$ANDROID_SERIAL" shell am instrument -w -r \
    -e class chat.omcp.app.PrivateUiCaptureTest \
    chat.auralis.app.uicapture.test/androidx.test.runner.AndroidJUnitRunner \
    | tee "$REPORTS/capture-$theme.txt"
  grep -Eq '^OK \(1 test\)' "$REPORTS/capture-$theme.txt"
  if grep -Eq 'FAILURES|INSTRUMENTATION_FAILED|Process crashed' "$REPORTS/capture-$theme.txt"; then exit 1; fi
  mkdir -p "app/build/private-ui-captures/$theme"
  "$ADB" -s "$ANDROID_SERIAL" pull /sdcard/Android/data/chat.auralis.app.uicapture/files/ui-captures/. \
    "app/build/private-ui-captures/$theme/"
done
# No capture APK is distributed. The emulator and its generated identities are discarded.
