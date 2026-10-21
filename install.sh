#!/bin/sh
# Install the rustable CLI from a GitHub release.
#
#   curl -fsSL https://github.com/krr-software-solutions/rustable/releases/latest/download/install.sh | sh
#
# Environment:
#   RUSTABLE_VERSION       the version to install, e.g. 0.1.2 (default: the latest release)
#   RUSTABLE_INSTALL_DIR   where to put the binary (default: ~/.local/bin)
#   RUSTABLE_TARGET        override the detected target, e.g. aarch64-unknown-linux-musl
#
# Linux (x86_64, aarch64; static, so any distro) and macOS (x86_64, arm64). It never uses sudo:
# point RUSTABLE_INSTALL_DIR somewhere writable. To uninstall, delete the binary.
#
# Everything is inside main(), called on the last line, so a partial download runs nothing.
set -eu

REPO="krr-software-solutions/rustable"

say() { printf 'rustable-install: %s\n' "$*" >&2; }
die() { say "error: $*"; exit 1; }
has() { command -v "$1" > /dev/null 2>&1; }

# The Rust target triple of the release archive for this machine.
detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "unsupported CPU architecture: $arch (prebuilt: x86_64, aarch64; or: cargo install rustable-sql-cli)" ;;
  esac
  case "$os" in
    Linux) echo "$arch-unknown-linux-musl" ;;
    Darwin)
      # An x86_64 shell under Rosetta still gets the native arm64 binary.
      if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2> /dev/null || echo 0)" = 1 ]; then
        arch=aarch64
      fi
      echo "$arch-apple-darwin"
      ;;
    *) die "unsupported OS: $os (prebuilt: Linux, macOS; or: cargo install rustable-sql-cli)" ;;
  esac
}

download() { # <url> <file>
  if has curl; then
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location --output "$2" "$1"
  elif has wget; then
    wget --quiet --output-document="$2" "$1"
  else
    die "need curl or wget"
  fi
}

sha256_of() { # <file>
  if has sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif has shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    die "need sha256sum or shasum to verify the download"
  fi
}

main() {
  has curl || has wget || die "need curl or wget"
  target="${RUSTABLE_TARGET:-$(detect_target)}"
  install_dir="${RUSTABLE_INSTALL_DIR:-$HOME/.local/bin}"
  version="${RUSTABLE_VERSION:-}"
  version="${version#v}"

  if [ -n "$version" ]; then
    base="https://github.com/$REPO/releases/download/v$version"
  else
    base="https://github.com/$REPO/releases/latest/download"
  fi
  archive="rustable-sql-$target.tar.gz"

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  say "downloading $archive (${version:-latest})"
  download "$base/$archive" "$tmp/$archive" || die "couldn't download $base/$archive"
  download "$base/$archive.sha256" "$tmp/$archive.sha256" || die "couldn't download $base/$archive.sha256"

  expected="$(cut -d ' ' -f 1 < "$tmp/$archive.sha256")"
  actual="$(sha256_of "$tmp/$archive")"
  [ "$expected" = "$actual" ] || die "checksum mismatch for $archive: expected $expected, got $actual"

  tar -xzf "$tmp/$archive" -C "$tmp"
  binary="$(find "$tmp" -type f -name rustable | head -n 1)"
  [ -n "$binary" ] || die "no rustable binary in $archive"

  mkdir -p "$install_dir" || die "can't create $install_dir (set RUSTABLE_INSTALL_DIR)"
  [ -w "$install_dir" ] || die "$install_dir isn't writable (set RUSTABLE_INSTALL_DIR)"
  # Copy then rename, so a running rustable is never left half-written.
  cp "$binary" "$install_dir/.rustable.tmp"
  chmod 755 "$install_dir/.rustable.tmp"
  mv -f "$install_dir/.rustable.tmp" "$install_dir/rustable"

  installed="$("$install_dir/rustable" --version 2> /dev/null || echo rustable)"
  say "installed $installed to $install_dir/rustable"
  case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
      say "$install_dir isn't on your PATH; add it to your shell's profile:"
      say "  export PATH=\"$install_dir:\$PATH\""
      ;;
  esac
}

main "$@"
