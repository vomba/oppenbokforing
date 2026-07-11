#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== M6 packaged smoke (integration mode) =="
npm run test:fixtures
npm test -- src/i18n/index.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test m6_milestone
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test golden_scenarios

if [[ "${TAURI_SMOKE:-0}" == "1" ]]; then
  echo "== Building desktop package =="
  npm run icons:dev
  if [[ -n "${TAURI_BUILD_ARGS:-}" ]]; then
    echo "Tauri build args: ${TAURI_BUILD_ARGS}"
    # shellcheck disable=SC2086
    npm run tauri:build -- ${TAURI_BUILD_ARGS}
  else
    npm run tauri:build
  fi

  APP_BUNDLE="$(find src-tauri/target/release/bundle/macos -maxdepth 1 -name '*.app' -print -quit)"
  DMG_BUNDLE="$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -name '*.dmg' -print -quit)"

  if [[ -z "${APP_BUNDLE}" ]]; then
    echo "ERROR: expected .app bundle under src-tauri/target/release/bundle/macos" >&2
    exit 1
  fi
  if [[ -z "${DMG_BUNDLE}" ]]; then
    echo "ERROR: expected .dmg under src-tauri/target/release/bundle/dmg" >&2
    exit 1
  fi

  echo "Verified app bundle: ${APP_BUNDLE}"
  echo "Verified dmg bundle: ${DMG_BUNDLE}"
  echo "Package build completed. Run manual install/launch checks on the built artifact."
else
  echo "Skipping Tauri package build (set TAURI_SMOKE=1 to enable)."
fi

echo "M6 smoke checks passed."
