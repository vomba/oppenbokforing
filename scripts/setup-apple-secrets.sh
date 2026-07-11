#!/usr/bin/env bash
# Validate local Apple signing materials and push GitHub Actions secrets.
#
# Usage:
#   ./scripts/setup-apple-secrets.sh --check          # diagnose only
#   ./scripts/setup-apple-secrets.sh --dry-run        # show gh secret set commands
#   ./scripts/setup-apple-secrets.sh                  # interactive upload
#
# Non-interactive (paths must exist):
#   APPLE_P12_PATH=~/Downloads/DeveloperID.p12 \
#   APPLE_P12_PASSWORD='...' \
#   APPLE_API_KEY_P8_PATH=~/Downloads/AuthKey_ABCD1234EF.p8 \
#   APPLE_API_KEY_ID=ABCD1234EF \
#   APPLE_API_ISSUER_ID=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx \
#   ./scripts/setup-apple-secrets.sh --apply
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:---apply}"
REPO="${GITHUB_REPO:-$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || true)}"

red() { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }

required_secrets=(
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  APPLE_SIGNING_IDENTITY
  APPLE_API_KEY
  APPLE_API_ISSUER
  APPLE_API_KEY_BASE64
)

check_keychain() {
  yellow "== Keychain signing identities =="
  local identities
  identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
  if [ -z "$identities" ]; then
    red "No code signing identities found in Keychain."
    return 1
  fi
  printf '%s\n' "$identities"

  if printf '%s\n' "$identities" | grep -q 'Developer ID Application:'; then
    green "Found Developer ID Application identity (required for beta .dmg)."
    printf '%s\n' "$identities" | grep 'Developer ID Application:' | head -1
    return 0
  fi

  red "Missing Developer ID Application certificate."
  yellow "You have Apple Development only — that cannot sign distributable macOS apps."
  yellow "Create one at https://developer.apple.com/account/resources/certificates/list"
  yellow "  → Certificates → + → Developer ID Application → CSR from Keychain Access"
  return 1
}

check_github_secrets() {
  yellow "== GitHub Actions secrets ($REPO) =="
  if [ -z "$REPO" ]; then
    red "Could not resolve GitHub repo. Run from a git clone with gh authenticated."
    return 1
  fi

  local configured=0
  for name in "${required_secrets[@]}"; do
    if gh secret list --repo "$REPO" 2>/dev/null | awk '{print $1}' | grep -qx "$name"; then
      green "  set   $name"
      configured=$((configured + 1))
    else
      red "  miss  $name"
    fi
  done

  if [ "$configured" -eq "${#required_secrets[@]}" ]; then
    green "All ${#required_secrets[@]} Apple secrets configured."
    return 0
  fi
  yellow "$configured / ${#required_secrets[@]} secrets configured."
  return 1
}

resolve_signing_identity() {
  if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    printf '%s' "$APPLE_SIGNING_IDENTITY"
    return 0
  fi
  security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/^[[:space:]]*[0-9]*) //p' \
    | grep '^Developer ID Application:' \
    | head -1
}

encode_file() {
  local path="$1"
  if [ ! -f "$path" ]; then
    red "File not found: $path" >&2
    return 1
  fi
  base64 <"$path" | tr -d '\n'
}

prompt_if_empty() {
  local var_name="$1"
  local prompt="$2"
  local current="${!var_name:-}"
  if [ -n "$current" ]; then
    return 0
  fi
  read -r -p "$prompt" "$var_name"
}

prompt_secret_if_empty() {
  local var_name="$1"
  local prompt="$2"
  local current="${!var_name:-}"
  if [ -n "$current" ]; then
    return 0
  fi
  read -r -s -p "$prompt" "$var_name"
  echo ""
}

apply_secrets() {
  local dry_run="${1:-false}"

  prompt_if_empty APPLE_P12_PATH "Path to exported Developer ID .p12: "
  prompt_secret_if_empty APPLE_P12_PASSWORD "Password used when exporting .p12: "
  prompt_if_empty APPLE_API_KEY_P8_PATH "Path to App Store Connect AuthKey_*.p8: "
  prompt_if_empty APPLE_API_KEY_ID "App Store Connect API Key ID: "
  prompt_if_empty APPLE_API_ISSUER_ID "App Store Connect Issuer ID (UUID): "

  local identity cert_b64 key_b64
  identity="$(resolve_signing_identity)"
  if [ -z "$identity" ]; then
    red "Could not resolve APPLE_SIGNING_IDENTITY. Export a Developer ID Application cert first."
    exit 1
  fi

  cert_b64="$(encode_file "$APPLE_P12_PATH")"
  key_b64="$(encode_file "$APPLE_API_KEY_P8_PATH")"

  yellow "== Values to upload =="
  echo "  APPLE_SIGNING_IDENTITY=$identity"
  echo "  APPLE_API_KEY=$APPLE_API_KEY_ID"
  echo "  APPLE_API_ISSUER=$APPLE_API_ISSUER_ID"
  echo "  APPLE_CERTIFICATE=<base64 ${#cert_b64} chars>"
  echo "  APPLE_API_KEY_BASE64=<base64 ${#key_b64} chars>"

  if [ "$dry_run" = true ]; then
    yellow "Dry run — no secrets written."
    return 0
  fi

  if [ -z "$REPO" ]; then
    red "Cannot upload without a GitHub repo."
    exit 1
  fi

  yellow "== Uploading secrets to $REPO =="
  printf '%s' "$cert_b64" | gh secret set APPLE_CERTIFICATE --repo "$REPO"
  printf '%s' "$APPLE_P12_PASSWORD" | gh secret set APPLE_CERTIFICATE_PASSWORD --repo "$REPO"
  printf '%s' "$identity" | gh secret set APPLE_SIGNING_IDENTITY --repo "$REPO"
  printf '%s' "$APPLE_API_KEY_ID" | gh secret set APPLE_API_KEY --repo "$REPO"
  printf '%s' "$APPLE_API_ISSUER_ID" | gh secret set APPLE_API_ISSUER --repo "$REPO"
  printf '%s' "$key_b64" | gh secret set APPLE_API_KEY_BASE64 --repo "$REPO"

  green "Done. Re-run with --check to verify."
}

case "$MODE" in
  --check)
    check_keychain || true
    echo ""
    check_github_secrets || true
    ;;
  --dry-run)
    apply_secrets true
    ;;
  --apply|"")
    apply_secrets false
    ;;
  *)
    red "Unknown mode: $MODE"
    echo "Usage: $0 [--check | --dry-run | --apply]"
    exit 2
    ;;
esac
