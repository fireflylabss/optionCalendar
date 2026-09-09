#!/usr/bin/env bash
# Bump packaging/aur for a tagged release (no makepkg required — CI-friendly).
# Usage: ./packaging/aur/bump.sh v0.1.0
#        ./packaging/aur/bump.sh 0.1.0
# Optional env: OPTIONSDK_VER=0.1.3 (defaults to _optionsdk_ver in PKGBUILD)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKGBUILD="$ROOT/packaging/aur/PKGBUILD"
SRCINFO="$ROOT/packaging/aur/.SRCINFO"

TAG="${1:?usage: bump.sh <version|vVersion>}"
VER="${TAG#v}"
CAL_URL="https://github.com/fireflylabss/optionCalendar/archive/refs/tags/v${VER}.tar.gz"

SDK_VER="${OPTIONSDK_VER:-}"
if [[ -z "$SDK_VER" ]]; then
  SDK_VER="$(sed -n 's/^_optionsdk_ver=//p' "$PKGBUILD" | head -1)"
fi
if [[ -z "$SDK_VER" ]]; then
  echo "Could not resolve optionSDK version" >&2
  exit 1
fi
SDK_URL="https://github.com/fireflylabss/optionSDK/archive/refs/tags/v${SDK_VER}.tar.gz"

wait_url() {
  local url="$1"
  echo "==> waiting for $url"
  for _ in $(seq 1 12); do
    if curl -fsI "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 5
  done
  echo "tarball not reachable: $url" >&2
  exit 1
}

wait_url "$CAL_URL"
wait_url "$SDK_URL"

echo "==> hashing tarballs"
CAL_SHA="$(curl -fsSL "$CAL_URL" | sha256sum | awk '{print $1}')"
SDK_SHA="$(curl -fsSL "$SDK_URL" | sha256sum | awk '{print $1}')"
echo "    optionCalendar sha256=$CAL_SHA"
echo "    optionSDK   sha256=$SDK_SHA"

echo "==> updating PKGBUILD → $VER (optionSDK $SDK_VER)"
sed -i "s/^pkgver=.*/pkgver=${VER}/" "$PKGBUILD"
sed -i "s/^pkgrel=.*/pkgrel=1/" "$PKGBUILD"
sed -i "s/^_optionsdk_ver=.*/_optionsdk_ver=${SDK_VER}/" "$PKGBUILD"
perl -i -0pe "s/sha256sums=\(\s*(?:'[^']*'|\n|\s)*\)/sha256sums=(\n  '${CAL_SHA}'\n  '${SDK_SHA}'\n)/s" "$PKGBUILD"

echo "==> writing .SRCINFO"
cat > "$SRCINFO" <<EOF
pkgbase = optioncalendar
	pkgdesc = Minimal local calendar CLI — Option family
	pkgver = ${VER}
	pkgrel = 1
	url = https://github.com/fireflylabss/optionCalendar
	arch = x86_64
	license = Apache-2.0
	makedepends = cargo
	depends = gcc-libs
	depends = glibc
	source = optioncalendar-${VER}.tar.gz::https://github.com/fireflylabss/optionCalendar/archive/refs/tags/v${VER}.tar.gz
	source = optionSDK-${SDK_VER}.tar.gz::https://github.com/fireflylabss/optionSDK/archive/refs/tags/v${SDK_VER}.tar.gz
	sha256sums = ${CAL_SHA}
	sha256sums = ${SDK_SHA}

pkgname = optioncalendar
EOF

echo "==> done (packaging/aur ready for AUR push)"
