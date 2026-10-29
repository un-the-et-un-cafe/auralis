#!/usr/bin/env bash
# Sourced by the launcher: job ownership must be checked in its parent shell.
# A deadline never changes the test result; diagnostics identify cleanup faults.
auralis_live_child() {
  local wanted=$1 pid
  for pid in $(jobs -pr) $(jobs -ps); do
    [[ "$pid" == "$wanted" ]] && return 0
  done
  return 1
}

auralis_stop_child() {
  local pid=$1 label=$2 attempts=${3:-25} interval=${4:-0.2} i
  [[ -n "$pid" ]] || return 0
  # Never signal a stale PID or a process that this shell does not own.
  auralis_live_child "$pid" || return 0
  kill -TERM "$pid" 2>/dev/null || true
  for ((i=0; i<attempts; i++)); do
    if ! auralis_live_child "$pid"; then wait "$pid" 2>/dev/null || true; return 0; fi
    sleep "$interval"
  done
  echo "Android cleanup: $label ignored TERM; escalating owned child $pid to KILL." >&2
  if auralis_live_child "$pid"; then kill -KILL "$pid" 2>/dev/null || true; fi
  for ((i=0; i<attempts; i++)); do
    if ! auralis_live_child "$pid"; then wait "$pid" 2>/dev/null || true; return 1; fi
    sleep "$interval"
  done
  # Do not wait indefinitely even after KILL (for example, uninterruptible I/O).
  echo "Android cleanup: $label did not exit within its cleanup deadline." >&2
  return 1
}

auralis_cleanup_android() {
  local adb=$1 serial=$2 emulator_pid=$3 logcat_pid=$4
  local adb_seconds=${5:-5} attempts=${6:-25} interval=${7:-0.2} failed=0
  auralis_stop_child "$logcat_pid" logcat "$attempts" "$interval" || failed=1
  if [[ -n "$emulator_pid" ]] && auralis_live_child "$emulator_pid"; then
    if ! timeout --signal=TERM --kill-after=1 "$adb_seconds" "$adb" -s "$serial" emu kill >/dev/null 2>&1; then
      echo 'Android cleanup: emulator console shutdown failed or exceeded its deadline.' >&2
      failed=1
    fi
    auralis_stop_child "$emulator_pid" emulator "$attempts" "$interval" || failed=1
  fi
  return "$failed"
}
