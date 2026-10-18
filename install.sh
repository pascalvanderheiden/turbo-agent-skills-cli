#!/usr/bin/env bash

VERSION="0.1.0"
REPOSITORY="pascalvanderheiden/turbo-agent-skills-cli"
TASC_TEMP_DIR=""

detect_target() {
  local operating_system architecture target=""
  operating_system="$(uname -s)"
  architecture="$(uname -m)"

  case "$operating_system" in
    Linux)
      case "$architecture" in
        x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
        *) ;;
      esac
      ;;
    Darwin)
      case "$architecture" in
        x86_64|amd64) target="x86_64-apple-darwin" ;;
        aarch64|arm64) target="aarch64-apple-darwin" ;;
        *) ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      case "$architecture" in
        x86_64|amd64) target="x86_64-pc-windows-msvc" ;;
        *) ;;
      esac
      ;;
    *) ;;
  esac

  if [[ -z "$target" ]]; then
    printf 'Unsupported operating system or CPU: %s %s\n' "$operating_system" "$architecture" >&2
    return 1
  fi
  printf '%s\n' "$target"
}

main() {
  set -euo pipefail

  local target binary_name version archive_name base_url expected actual install_dir
  target="$(detect_target)" || return 1
  version="${TASC_VERSION:-$VERSION}"
  if [[ ! "$version" =~ ^v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    printf 'Invalid TASC version: %s\n' "$version" >&2
    return 1
  fi
  version="v${version#v}"
  binary_name="tasc"
  [[ "$target" == *-msvc ]] && binary_name="tasc.exe"
  archive_name="tasc-${version}-${target}.tar.gz"
  base_url="https://github.com/${REPOSITORY}/releases/download/${version}"

  if ! command -v curl >/dev/null 2>&1; then
    printf 'curl is required to download TASC.\n' >&2
    return 1
  fi
  if [[ -z "${HOME:-}" ]]; then
    printf 'HOME must be set to choose a user-writable install directory.\n' >&2
    return 1
  fi

  TASC_TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tasc-install.XXXXXX")" || {
    printf 'Could not create a temporary directory for the download.\n' >&2
    return 1
  }
  trap 'rm -rf "$TASC_TEMP_DIR"' EXIT

  curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    "${base_url}/${archive_name}" --output "${TASC_TEMP_DIR}/${archive_name}"
  curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    "${base_url}/SHA256SUMS.txt" --output "${TASC_TEMP_DIR}/SHA256SUMS.txt"

  expected="$(awk -v filename="$archive_name" \
    '$2 == filename { print $1; count++ } END { if (count != 1) exit 1 }' \
    "${TASC_TEMP_DIR}/SHA256SUMS.txt")" || {
    printf 'SHA256SUMS.txt has no unique checksum for %s.\n' "$archive_name" >&2
    return 1
  }
  if [[ ! "$expected" =~ ^[[:xdigit:]]{64}$ ]]; then
    printf 'SHA256SUMS.txt contains an invalid checksum for %s.\n' "$archive_name" >&2
    return 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "${TASC_TEMP_DIR}/${archive_name}" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "${TASC_TEMP_DIR}/${archive_name}" | awk '{print $1}')"
  else
    printf 'sha256sum or shasum is required to verify the TASC download.\n' >&2
    return 1
  fi
  if [[ "${actual,,}" != "${expected,,}" ]]; then
    printf 'SHA-256 verification failed for %s.\n' "$archive_name" >&2
    return 1
  fi

  if ! tar -tzf "${TASC_TEMP_DIR}/${archive_name}" | grep -Fxq "$binary_name"; then
    printf 'The verified archive does not contain the expected %s executable.\n' "$binary_name" >&2
    return 1
  fi
  tar -xzf "${TASC_TEMP_DIR}/${archive_name}" -C "$TASC_TEMP_DIR" "$binary_name"
  install_dir="${TASC_INSTALL_DIR:-${HOME}/.local/bin}"
  if ! mkdir -p "$install_dir" || [[ ! -d "$install_dir" || ! -w "$install_dir" ]]; then
    printf 'Install directory is not writable: %s\n' "$install_dir" >&2
    return 1
  fi
  if ! install -m 755 "${TASC_TEMP_DIR}/${binary_name}" "${install_dir}/${binary_name}"; then
    printf 'Could not install TASC into %s.\n' "$install_dir" >&2
    return 1
  fi
  printf 'Installed TASC %s to %s/%s\n' "$version" "$install_dir" "$binary_name"
  case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *) printf 'Add %s to your PATH to run tasc.\n' "$install_dir" ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
