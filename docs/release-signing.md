# Signing releases

`.github/workflows/build.yml` builds the installers on `v*` tags and on manual dispatch. This page explains how release builds get the closed-source Sheafly Pro modules, how the macOS app gets signed with a Developer ID and notarized by Apple, and what is still open for Windows.

## Sheafly Pro modules

New Pro features live in the private repository `hoanghainh1188/sheafly-pro` (plan part 20G). This repository has an MIT stub of that crate at `crates/pro`, so it builds on its own; release builds put the private crate in its place.

- The `pro` job checks out `sheafly-pro` (its `main`) with the read-only deploy key in the secret `SHEAFLY_PRO_DEPLOY_KEY`, runs its `scripts/check-stub.py` against `crates/pro` (same dependencies and the same `tests/api.rs`, so `Cargo.lock` and `THIRD_PARTY_LICENSES.txt` stay true) and passes that commit to every platform, which writes it over `crates/pro` before building. The commit is shown in the run's summary.
- **No key** (forks, or a manual run before it is set): the installers are built with the stub and have no Pro features; the run shows a warning. **On a `v*` tag** the `pro` job fails instead, so a release never ships without the Pro modules.
- Nothing from the private repository is uploaded as an artifact (artifacts of a public repository can be downloaded by anyone), and the key is not kept in any git config.
- A change of the stub's functions or dependencies must reach `sheafly-pro`'s `main` before a release, or the check fails.
- **Every pull request and push to `main`** also runs the `pro-stub` job of `.github/workflows/ci.yml` with the same key: it compares `crates/pro` with the `sheafly-pro` branch of the same name as the PR's branch (the convention for PR pairs), else with its `main`. A public PR that changes the stub stays red until its private side is pushed, and a public PR merged before its private one turns `main` red. To make GitHub refuse such a merge, add the `pro-stub` check to the required status checks of `main` (Settings → Branches, or Rules → Rulesets). Forks have no key and skip it.

### Setting up the deploy key

1. Make a key pair with no passphrase: `ssh-keygen -t ed25519 -N "" -C "sheafly release builds" -f sheafly-pro-deploy`.
2. In `sheafly-pro` → Settings → Deploy keys → Add deploy key: paste `sheafly-pro-deploy.pub`, leave **Allow write access** off.
3. In this repository → Settings → Secrets and variables → Actions → New repository secret: name `SHEAFLY_PRO_DEPLOY_KEY`, value the whole of `sheafly-pro-deploy` (the private key, `BEGIN`/`END` lines included).
4. Delete both files from your computer. A lost or leaked key is replaced by repeating these steps and removing the old deploy key; users are not affected.

## macOS

### How the workflow decides

The macOS job signs and notarizes only when **all six** repository secrets below are set:

| Secret | What it holds |
|---|---|
| `APPLE_CERTIFICATE` | The Developer ID Application certificate and its private key, exported as `.p12`, in base64 |
| `APPLE_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate's name, e.g. `Developer ID Application: Jane Doe (AB12CD34EF)` |
| `APPLE_API_ISSUER` | The Issuer ID of the App Store Connect API key (a UUID) |
| `APPLE_API_KEY` | The Key ID of that API key (10 characters) |
| `APPLE_API_PRIVATE_KEY` | The contents of the key's `AuthKey_<KeyID>.p8` file, including the `BEGIN`/`END` lines |

- **None set** (today, and in every fork): the app keeps the ad-hoc signature from `bundle.macOS.signingIdentity "-"` in `src-tauri/tauri.conf.json`. macOS shows "Open Anyway" in System Settings instead of calling the app damaged. The release text says the build is not signed.
- **All set**: a step exports them for the Tauri CLI, writes the `.p8` to a private temporary file (`APPLE_API_KEY_PATH`), and the CLI signs the app with the hardened runtime, notarizes it with `notarytool` and staples the ticket. `APPLE_SIGNING_IDENTITY` overrides the `"-"` in the config, so the config does not change. The release text says the macOS app is signed and notarized.
- **Some set**: the job fails and lists the missing ones, so a half-configured repository never ships an ad-hoc app by accident.

After the build, the job mounts the universal `.dmg` and checks the app inside:

- always: `lipo -archs` on the main binary must list both `x86_64` and `arm64`;
- when signed: `codesign --verify --strict --deep`, `spctl -a -vv -t exec` (must report `source=Notarized Developer ID`) and `xcrun stapler validate`.

No entitlements file is needed. The app is not sandboxed, and under the hardened runtime WKWebView runs JavaScript in Apple's own WebContent process, so the app needs no JIT exception. The tracker's HTTPS calls and the keychain (`keyring`) work without entitlements outside the sandbox. If a later feature needs one (camera, Apple Events, a JIT of our own), add `bundle.macOS.entitlements` and run the checks again.

### One-time setup

You need a paid Apple Developer Program membership (individual or organization). Only the Account Holder can create Developer ID certificates.

#### 1. Developer ID Application certificate

1. On a Mac, open **Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority…**. Enter your email, choose **Saved to disk**, and save the `.certSigningRequest`.
2. At <https://developer.apple.com/account/resources/certificates/add> choose **Developer ID Application** (not "Developer ID Installer", not "Apple Distribution"), pick the **G2 Sub-CA** profile, upload the request and download the `.cer`.
3. Double-click the `.cer` to add it to the login keychain. In **My Certificates** it now shows with its private key under it.
4. Find the exact name for `APPLE_SIGNING_IDENTITY`:

   ```bash
   security find-identity -v -p codesigning
   ```

   Copy the quoted text, e.g. `Developer ID Application: Jane Doe (AB12CD34EF)`.

#### 2. Export it as `.p12` and base64

1. In Keychain Access, select the certificate **and** its private key, right-click → **Export 2 items…**, save as `sheafly-developer-id.p12` and set a strong password (that is `APPLE_CERTIFICATE_PASSWORD`).
2. Encode it on one line and copy it:

   ```bash
   base64 -i sheafly-developer-id.p12 | pbcopy
   ```

   Paste it as `APPLE_CERTIFICATE`. Delete the `.p12` afterwards or keep it in a password manager; it is the signing key.

#### 3. App Store Connect API key for notarization

1. At <https://appstoreconnect.apple.com/access/integrations/api> (Users and Access → Integrations → App Store Connect API) request access if asked, then **Generate API Key** under **Team Keys**. A role of **Developer** is enough for notarization.
2. Note the **Issuer ID** at the top of the page (`APPLE_API_ISSUER`) and the key's **Key ID** (`APPLE_API_KEY`).
3. Download `AuthKey_<KeyID>.p8`. Apple lets you download it only once. Paste its whole contents as `APPLE_API_PRIVATE_KEY`.

#### 4. Add the secrets

In the repository: **Settings → Secrets and variables → Actions → New repository secret**, one per name in the table. Or with the GitHub CLI:

```bash
base64 -i sheafly-developer-id.p12 | gh secret set APPLE_CERTIFICATE
```

```bash
gh secret set APPLE_API_PRIVATE_KEY < AuthKey_AB12CD34EF.p8
```

(`gh secret set NAME` with no input asks for the value.) Then run **Actions → Build installers → Run workflow** on `main` and check that the macOS job's "Check the macOS app" step runs the signed checks.

### Checking a build locally

On a Mac, with the `.dmg` from the workflow's artifacts or the draft release:

```bash
hdiutil attach -nobrowse -readonly Sheafly_1.5.0_universal.dmg
```

```bash
codesign --verify --strict --deep --verbose=2 /Volumes/Sheafly/Sheafly.app
```

```bash
spctl -a -vv -t exec /Volumes/Sheafly/Sheafly.app
```

```bash
xcrun stapler validate /Volumes/Sheafly/Sheafly.app
```

```bash
lipo -archs /Volumes/Sheafly/Sheafly.app/Contents/MacOS/sheafly
```

`spctl` should print `accepted` and `source=Notarized Developer ID`; an ad-hoc build prints `rejected`. The volume and binary names follow the `.dmg`; adjust them if they differ. To see why a notarization failed, ask `notarytool` with the same API key:

```bash
xcrun notarytool history --key AuthKey_AB12CD34EF.p8 --key-id AB12CD34EF --issuer <issuer-id>
```

```bash
xcrun notarytool log <submission-id> --key AuthKey_AB12CD34EF.p8 --key-id AB12CD34EF --issuer <issuer-id>
```

To sign and notarize a local build instead of CI, export the same variables (with `APPLE_API_KEY_PATH` pointing at the `.p8`) and run `pnpm tauri build --target universal-apple-darwin`.

### Renewing

A Developer ID Application certificate is valid for five years; apps signed and notarized before it expires keep opening. Before it expires, create a new one as above and replace `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY`. If the API key is revoked or leaks, revoke it in App Store Connect, create a new one and replace the three `APPLE_API_*` secrets.

## Windows (not signed yet)

The Windows installers stay unsigned for now, so SmartScreen warns on first run ("Windows protected your PC" → More info → Run anyway). Options for later:

- **Azure Trusted Signing** (Microsoft's managed signing service): a monthly fee, no hardware token, signs from CI with an Azure identity. Availability depends on the publisher's country and on identity validation, so check that first. Tauri can call it through `bundle.windows.signCommand`.
- **An OV code-signing certificate through a cloud signing service** (e.g. a CA's cloud HSM such as SSL.com eSigner, DigiCert KeyLocker, Certum SimplySign): since 2023 these keys must live on hardware, so a cloud service is the practical way to sign from CI. Also wired through `bundle.windows.signCommand`.

Either way SmartScreen reputation builds up over downloads; an EV certificate no longer skips that. Linux packages are not signed; `SHA256SUMS.txt` on each release lets people check the downloads.
