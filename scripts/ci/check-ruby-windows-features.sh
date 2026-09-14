#!/usr/bin/env bash
# Assert the Windows Ruby gem's two feature switches agree.
#
# The Windows leg builds xberg-rb with an explicit `--features` list while the Windows target
# dependency asks the core for the `windows-gnu-target` bundle. Those are separate switches:
# the binding's own features decide which `#[cfg(feature = "...")]`-gated match arms, struct
# fields and registrations alef emits, while the bundle decides which variants and fields the
# core actually has. When a core feature is enabled by the bundle and its same-named binding
# feature is NOT selected, the core keeps the variant and the binding loses the arm, and the
# generated conversions stop compiling. That has now broken the release twice -- 827 E0004
# errors once, 453 errors the next time -- and neither was visible on the Linux or macOS legs.
#
# Cargo features are additive, so this cannot be fixed by narrowing one side; the two lists
# have to be kept in step, and this is what makes "in step" checkable instead of assumed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
core_manifest="${repo_root}/crates/xberg/Cargo.toml"
binding_manifest="${repo_root}/packages/ruby/ext/xberg_rb/native/Cargo.toml"
workflow="${repo_root}/.github/workflows/publish.yaml"

for required in "$core_manifest" "$binding_manifest" "$workflow"; do
  [ -f "$required" ] || {
    echo "error: missing $required" >&2
    exit 1
  }
done

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

awk '/^windows-gnu-target = \[/,/^\]/' "$core_manifest" |
  grep -oE '"[a-z0-9-]+"' | tr -d '"' | sort -u >"$work/bundle"
awk '/^\[features\]/,/^\[(dependencies|target)/' "$binding_manifest" |
  grep -oE '^[a-z0-9-]+ = \[' | sed 's/ = \[//' | sort -u >"$work/binding"
grep "ruby_cargo_args" "$workflow" | sed 's/.*--features //' |
  tr ',' '\n' | tr -d ' ' | grep -E '^[a-z0-9-]+$' | sort -u >"$work/selected"

# Each set must be non-empty. An extraction that silently matched nothing would make every
# comparison below trivially pass -- the same shape of vacuous green this check exists to catch.
for name in bundle binding selected; do
  count="$(grep -c . "$work/$name" || true)"
  if [ "${count:-0}" -eq 0 ]; then
    echo "error: extracted 0 entries for '${name}' -- this check parsed nothing and proved nothing" >&2
    exit 1
  fi
  echo "ruby windows feature check: ${name}=${count}"
done

comm -12 "$work/bundle" "$work/binding" | comm -23 - "$work/selected" >"$work/missing"
if [ -s "$work/missing" ]; then
  echo "error: windows-gnu-target enables these core features whose xberg-rb counterparts are NOT" >&2
  echo "       in ruby_cargo_args, so the generated conversions will not compile:" >&2
  sed 's/^/  - /' "$work/missing" >&2
  echo "Add them to the Windows leg's ruby_cargo_args in .github/workflows/publish.yaml." >&2
  exit 1
fi

echo "ruby windows feature check: ok -- every bundled core feature with a binding counterpart is selected"
