# Apple code signing setup (optional — deferred)

> **Status (2026-07-11):** Not required for current use. The app ships for local-first personal use via `npm run tauri:dev` or an unsigned `npm run tauri build` artifact. Paid Apple Developer Program membership (~$99/year) is **deferred** until there is a clear need for wide public distribution.
>
> **Alternatives today:**
> - **Dev loop:** `npm run tauri:dev` (no signing)
> - **Local install:** unsigned `.app` / `.dmg` from `npm run tauri build` — Gatekeeper may require right-click → Open on first launch
> - **Trusted handoff:** zip the `.app` and share with people who already trust you (same Gatekeeper caveat)
> - **Future:** self-hosted update manifest without App Store; revisit signing only if macOS blocks become a real support burden

Keep this document for **if/when** signed releases are worth the cost. The CI workflow (`.github/workflows/release-macos.yml`) remains ready but is not on the critical path.

---

Configure these GitHub Actions secrets before tagging a signed beta release.

## Quick start (when you choose to sign later)

```sh
chmod +x scripts/setup-apple-secrets.sh
./scripts/setup-apple-secrets.sh --check    # diagnose keychain + GitHub secrets
./scripts/setup-apple-secrets.sh            # interactive upload via gh secret set
```

## Prerequisites

- macOS with Xcode command-line tools
- Apple Developer Program membership (paid — only if you proceed)
- `gh` CLI authenticated to the repo

## 0. Create Developer ID Application certificate (one-time)

1. Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority
2. Save CSR to disk
3. [developer.apple.com](https://developer.apple.com/account/resources/certificates/list) → **+** → **Developer ID Application** → upload CSR → download `.cer`
4. Double-click `.cer` to install in login keychain

Verify:

```sh
security find-identity -v -p codesigning | grep 'Developer ID Application'
```

## 1. Export signing certificate (.p12)

1. Keychain Access → My Certificates → **Developer ID Application: …**
2. Right-click → Export → `.p12` with a strong export password
3. Base64-encode for GitHub secret:

```sh
base64 -i ~/Desktop/DeveloperID.p12 | pbcopy
# Paste into APPLE_CERTIFICATE secret
```

List identities:

```sh
security find-identity -v -p codesigning
```

Set `APPLE_SIGNING_IDENTITY` to the full name, e.g. `Developer ID Application: Your Name (TEAMID)`.

## 2. App Store Connect API key (notarization)

1. [App Store Connect](https://appstoreconnect.apple.com) → Users and Access → Integrations → API Keys
2. Create key with **Developer** role
3. Download `.p8` (once) and note Key ID + Issuer ID

Secrets:

| Secret | Value |
|--------|-------|
| `APPLE_API_KEY` | App Store Connect API Key ID |
| `APPLE_API_KEY_BASE64` | Base64 of `.p8` file |
| `APPLE_API_ISSUER` | Issuer ID (UUID) |
| `APPLE_CERTIFICATE` | Base64 of `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Export password |
| `APPLE_SIGNING_IDENTITY` | Full codesign identity string |

## 3. Upload secrets (automated)

```sh
./scripts/setup-apple-secrets.sh --apply
```

## 4. Tag a release (only when signing is configured)

```sh
git tag v0.1.0-beta.1
git push origin v0.1.0-beta.1
```

Release pipeline:

1. **release-gates** — `test:all` + packaged smoke on `universal-apple-darwin`
2. **release-macos** — sign, notarize, attach `.dmg`

## 5. Post-release smoke

3. Run manual smoke from `docs/security-review-beta.md`:

- Launch signed `.dmg`, open workspace, create invoice, encrypted backup round-trip

## Related docs

- `docs/packaging-release.md`
- `docs/security-review-beta.md`
