#!/usr/bin/env bash

set -euo pipefail

target="${TARGET:?TARGET not set}"

case "$target" in
x86_64-unknown-linux-gnu) node_file="xberg-node.linux-x64-gnu.node" ;;
aarch64-unknown-linux-gnu) node_file="xberg-node.linux-arm64-gnu.node" ;;
*)
  echo "verify-glibc-floor: target $target is not a linux-gnu prebuild — skipping" >&2
  exit 0
  ;;
esac

node_path="crates/xberg-node/artifacts/${node_file}"
if [ ! -f "$node_path" ]; then
  echo "verify-glibc-floor: ${node_path} not found" >&2
  exit 1
fi

if ! command -v objdump >/dev/null 2>&1; then
  echo "verify-glibc-floor: objdump not available on runner" >&2
  exit 1
fi

MAX_FLOOR="GLIBC_2.28"
failed=0

echo "=== Symbol audit: ${node_path} ==="

dynsyms=$(objdump -T "$node_path")

max_glibc=$(printf '%s\n' "$dynsyms" | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)*' | sort -uV | tail -1 || true)
if [ -z "$max_glibc" ]; then
  echo "  FAIL: no GLIBC_* version symbols found in ${node_file}."
  echo "  A linux-gnu prebuild should always reference at least one versioned glibc"
  echo "  symbol; an empty result means the audit can't detect floor drift and is"
  echo "  almost certainly looking at the wrong file or a corrupted artifact."
  failed=1
else
  highest=$(printf '%s\n%s\n' "$max_glibc" "$MAX_FLOOR" | sort -V | tail -1)
  if [ "$highest" != "$MAX_FLOOR" ]; then
    echo "  FAIL: ${node_file} requires ${max_glibc} (> ${MAX_FLOOR})"
    echo "  This breaks @xberg-io/xberg on RHEL 8 / AlmaLinux 8 / Rocky 8."
    echo "  Likely cause: a Rust dependency has been bumped to a version that"
    echo "  references a newer glibc symbol; revert the bump or stay on this floor."
    failed=1
  else
    echo "  OK: max glibc symbol = ${max_glibc} (≤ ${MAX_FLOOR})"
  fi
fi

glibcxx=$(printf '%s\n' "$dynsyms" | grep -oE 'GLIBCXX_[0-9]+(\.[0-9]+)*' | sort -uV || true)
if [ -n "$glibcxx" ]; then
  echo "  FAIL: ${node_file} references GLIBCXX symbols:"
  while IFS= read -r line; do printf '    %s\n' "$line"; done <<<"$glibcxx"
  echo "  zig is supposed to bundle libstdc++ statically; this means the build"
  echo "  switched off zigbuild or zig's runtime is being shadowed by the host."
  failed=1
else
  echo "  OK: no GLIBCXX_* references (libstdc++ bundled by zig)"
fi

isoc23=$(printf '%s\n' "$dynsyms" | grep -E '__isoc23_' || true)
if [ -n "$isoc23" ]; then
  echo "  FAIL: ${node_file} references C23 glibc helpers:"
  while IFS= read -r line; do printf '    %s\n' "$line"; done <<<"$isoc23"
  echo "  This requires glibc ≥ 2.38 and breaks the floor."
  failed=1
else
  echo "  OK: no __isoc23_* references"
fi

if [ "$failed" -eq 1 ]; then
  echo
  echo "Symbol audit FAILED for ${node_file} — refusing to package and publish." >&2
  exit 1
fi

echo
echo "Symbol audit PASSED for ${node_file}."
