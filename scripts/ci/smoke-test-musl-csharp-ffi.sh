#!/usr/bin/env bash
# Builds a small standalone .NET program against the staged musl xberg-ffi
# native closure and runs a real extraction against a known fixture inside a
# genuine Alpine (musl libc) container, asserting the exact expected text
# comes back. See scripts/ci/fixtures/csharp-smoke/Program.cs for why a
# P/Invoke-resolves check is not sufficient here (same rationale as
# xberg-io/xberg#490 for the Python wheel).
#
# Deliberately does NOT run on the GitHub Actions host directly: the host is
# glibc (ubuntu-latest / ubuntu-24.04-arm), and the entire point of the gate
# is proving the library works on the musl libc family it was cross-compiled
# for.
set -euo pipefail

log() { echo "smoke-test-musl-csharp-ffi: $*" >&2; }
die() {
  log "$*"
  exit 1
}

NATIVE_DIR="${1:?usage: $0 <dir-containing-libxberg_ffi.so-and-vendored-.so-closure>}"
[ -d "$NATIVE_DIR" ] || die "native asset directory not found: $NATIVE_DIR"
# `docker -v` rejects a relative source as a volume NAME ("includes invalid
# characters for a local volume name"), so this must be absolute before the
# mount below. Matches smoke-test-musl-python-wheel.sh, which already does it. ~keep
NATIVE_DIR="$(cd "$NATIVE_DIR" && pwd)"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="$REPO_ROOT/scripts/ci/fixtures/musl-python-smoke.pdf"
SMOKE_DIR="$REPO_ROOT/scripts/ci/fixtures/csharp-smoke"
EXPECTED_TEXT="XBERG MUSL SMOKE 490"

[ -f "$FIXTURE" ] || die "smoke-test fixture missing: $FIXTURE"
[ -f "$SMOKE_DIR/Smoke.csproj" ] || die "smoke-test project missing: $SMOKE_DIR/Smoke.csproj"
find "$NATIVE_DIR" -maxdepth 1 -name 'libxberg_ffi.so' | grep -q . || die "no libxberg_ffi.so found under $NATIVE_DIR"

# mcr.microsoft.com/dotnet/sdk is Microsoft's official musl-libc SDK image
# (Alpine-based), so this is the same musl libc family the library was
# cross-compiled for.
docker run --rm \
  -v "$REPO_ROOT:/repo:ro" \
  -v "$NATIVE_DIR:/native:ro" \
  -v "$FIXTURE:/fixture.pdf:ro" \
  -e EXPECTED_TEXT="$EXPECTED_TEXT" \
  mcr.microsoft.com/dotnet/sdk:10.0-alpine sh -euc '
    cp -r /repo /work
    dotnet build -c Release /work/scripts/ci/fixtures/csharp-smoke/Smoke.csproj -o /out
    cp /native/*.so* /out/
    dotnet exec /out/Smoke.dll /fixture.pdf "${EXPECTED_TEXT}"
  '
