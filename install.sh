#!/usr/bin/env bash

VERSION="0.2.0"
REPOSITORY="pascalvanderheiden/turbo-agent-skills-cli"
TASC_TEMP_DIR=""

presentation_event() {
  local category="$1" body="$2" glyph label color fd=1
  case "$category" in
    Input) glyph="›"; color="36" ;;
    TASC) glyph="◆"; color="35" ;;
    Prompt) glyph="?"; color="36"; fd=2 ;;
    Info) glyph="·"; color="34" ;;
    Working) glyph="..."; color="36"; fd=2 ;;
    Warning) glyph="!"; color="33"; fd=2 ;;
    Error) glyph="×"; color="31"; fd=2 ;;
    Done) glyph="✓"; color="32" ;;
    Detecting|Downloading|Verifying|Installing) glyph="..."; color="36"; fd=2 ;;
    Installed) glyph="✓"; color="32" ;;
    Next) glyph="↳"; color="2;36" ;;
    *) return 1 ;;
  esac
  label="${category^^}:"
  if [[ -t "$fd" && ! ${NO_COLOR+x} && "${TERM:-}" != "dumb" ]]; then
    label="$(printf '\033[%sm%s %s\033[0m' "$color" "$glyph" "$category")"
  fi
  body="${body//$'\033'/\\e}"
  body="${body//$'\r'/\\r}"
  printf '%s %s\n' "$label" "$body" >&"$fd"
}

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
    presentation_event Error "Unsupported operating system or CPU: $operating_system $architecture"
    return 1
  fi
  printf '%s\n' "$target"
}

main() {
  set -euo pipefail

  local target binary_name version archive_name base_url expected actual install_dir
  presentation_event Detecting "platform"
  target="$(detect_target)" || return 1
  version="${TASC_VERSION:-$VERSION}"
  if [[ ! "$version" =~ ^v?(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    presentation_event Error "Invalid TASC version: $version"
    return 1
  fi
  version="v${version#v}"
  binary_name="tasc"
  [[ "$target" == *-msvc ]] && binary_name="tasc.exe"
  archive_name="tasc-${version}-${target}.tar.gz"
  base_url="https://github.com/${REPOSITORY}/releases/download/${version}"

  if ! command -v curl >/dev/null 2>&1; then
    presentation_event Error "curl is required to download TASC."
    return 1
  fi
  if [[ -z "${HOME:-}" ]]; then
    presentation_event Error "HOME must be set to choose a user-writable install directory."
    return 1
  fi

  TASC_TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tasc-install.XXXXXX")" || {
    presentation_event Error "Could not create a temporary directory for the download."
    return 1
  }
  trap 'rm -rf "$TASC_TEMP_DIR"' EXIT

  presentation_event Downloading "$archive_name"
  if ! curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    "${base_url}/${archive_name}" --output "${TASC_TEMP_DIR}/${archive_name}" ||
    ! curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
      "${base_url}/SHA256SUMS.txt" --output "${TASC_TEMP_DIR}/SHA256SUMS.txt"; then
    presentation_event Error "Download failed for $archive_name."
    return 1
  fi

  presentation_event Verifying "checksum and archive"
  expected="$(awk -v filename="$archive_name" \
    '$2 == filename { print $1; count++ } END { if (count != 1) exit 1 }' \
    "${TASC_TEMP_DIR}/SHA256SUMS.txt")" || {
    presentation_event Error "SHA256SUMS.txt has no unique checksum for $archive_name."
    return 1
  }
  if [[ ! "$expected" =~ ^[[:xdigit:]]{64}$ ]]; then
    presentation_event Error "SHA256SUMS.txt contains an invalid checksum for $archive_name."
    return 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "${TASC_TEMP_DIR}/${archive_name}" | awk '{print $1}')" || {
      presentation_event Error "Could not calculate the SHA-256 checksum for $archive_name."
      return 1
    }
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "${TASC_TEMP_DIR}/${archive_name}" | awk '{print $1}')" || {
      presentation_event Error "Could not calculate the SHA-256 checksum for $archive_name."
      return 1
    }
  else
    presentation_event Error "sha256sum or shasum is required to verify the TASC download."
    return 1
  fi
  if [[ "${actual,,}" != "${expected,,}" ]]; then
    presentation_event Error "SHA-256 verification failed for $archive_name."
    return 1
  fi

  if ! tar -tzf "${TASC_TEMP_DIR}/${archive_name}" | grep -Fxq "$binary_name"; then
    presentation_event Error "The verified archive does not contain the expected $binary_name executable."
    return 1
  fi
  if ! tar -xzf "${TASC_TEMP_DIR}/${archive_name}" -C "$TASC_TEMP_DIR" "$binary_name"; then
    presentation_event Error "Could not extract $binary_name from the verified archive."
    return 1
  fi
  install_dir="${TASC_INSTALL_DIR:-${HOME}/.local/bin}"
  presentation_event Installing "$install_dir"
  if ! mkdir -p "$install_dir" || [[ ! -d "$install_dir" || ! -w "$install_dir" ]]; then
    presentation_event Error "Install directory is not writable: $install_dir"
    return 1
  fi
  if ! install -m 755 "${TASC_TEMP_DIR}/${binary_name}" "${install_dir}/${binary_name}"; then
    presentation_event Error "Could not install TASC into $install_dir."
    return 1
  fi
  presentation_event Installed "TASC $version to $install_dir/$binary_name"
  case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *) presentation_event Next "Add $install_dir to your PATH to run tasc." ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
