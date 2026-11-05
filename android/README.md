# Android apps

Dispatch's Android apps, in one Gradle build: each app in a folder of its own, and anything they
come to share in one beside them. Developed on the MacBook, with Android Studio's SDK. CI's
`android` job (`.github/workflows/checks.yml`) runs their unit tests, lint and debug builds when
a change touches this folder, and passes at once for any other.

- `relay/`: **DSP Dispatch: Relay**, the app on a DSP's dispatch phone. It relays the phone's
  texts and pictures to the DSP's Dispatch and sends Dispatch's from the phone's number, as
  [the phones' protocol](../features/communications/api/phone/README.md) has it.

## Building

From this folder, with the SDK at `~/Library/Android/sdk` (`ANDROID_HOME`) and Java 25:

```sh
./gradlew test lintDevDebug lintProductionRelease assembleDebug  # what CI runs
./gradlew installDevDebug  # the dev app, debug build, on the phone or emulator adb sees
```

Every app's tests run on Gradle's Java through `gradle/capped-jdk`, which caps any file they write
at 1 GB, so a test that writes without end fails on its own write. Their temp folder is the app's
`build/tmp/test-scratch`, emptied before each run, and they stop after 15 minutes. A test
never feeds code that writes to disk an input that never ends; one just past the limit proves it.

## Relay's builds

One app for each server, from the same source and the same commit:

| Build        | App ID                      | Name                    | Pairs with                       |
| ------------ | --------------------------- | ----------------------- | -------------------------------- |
| `dev`        | `com.dspdispatch.relay.dev` | DSP Dispatch: Relay Dev | DSPs anywhere but Production's   |
| `production` | `com.dspdispatch.relay`     | DSP Dispatch: Relay     | `https://<code>.dspdispatch.com` |

Each version reaches Dev first, to test; Production serves it only once a release includes it.
A debug build adds `.debug` to its app ID, so it installs beside the signed app, never in its
place. App IDs never change once phones have the app.

## Signing

Release builds are signed in CI alone, each with its server's key, kept as secrets of a GitHub
environment and never in the repo: `relay-dev` holds the Dev key, for `main` and
`release-checks/v*`, and `relay-production` the Production key, for `release-checks/v*` alone.
Each has `RELAY_KEYSTORE` (the PKCS12 keystore in base64), `RELAY_KEYSTORE_PASSWORD`,
`RELAY_KEY_ALIAS` and `RELAY_KEY_PASSWORD`. A copy of each key is in the owner's password manager:
a phone only updates to an APK signed with the key it was installed with. Gradle signs a release
build when `RELAY_KEYSTORE_FILE` names the keystore, with the other three beside it; without it, a
release builds unsigned.

## Versions

A build's version comes from the build, never the source:
`./gradlew -Prelay.versionCode=<n> -Prelay.versionName=<name> …`. Without them it's versionCode 1,
named `0.0.0-local`, clearly no release.

- `versionCode`: the commit count of the built commit on `main` (`git rev-list --count HEAD`), the
  same for its Dev and Production builds, higher with each commit.
- Production's name: the app version, three numbers from `0.0.1`, which goes up only with a
  release that changes the app: its next patch, or more with the release's `--bump`. A release
  without app changes carries the last released app unchanged.
- Dev's name: `<next app version>-dev.<versionCode>`, as `0.0.2-dev.1372`, the next app version
  being the last released one's next patch, `0.0.1` before any.

## Releases

CI builds and signs both apps (`.github/workflows/checks.yml`, with `tooling/ci/relay.py`):

- **Each commit of `main`:** `relay-dev` builds Dev's app (the `dev` flavor's release build),
  signed with the Dev key. It builds again for each commit that changes `android/`, a revert
  too, and when the next app version or the key changes; otherwise the signed APK of the same
  inputs comes from the Actions cache, while it still holds it, so a change elsewhere rarely waits
  on Gradle. `build` packs it in the runtime package, which Dev installs as it does each commit
  of `main`.
- **A release:** the release command dispatches the checks for its commit on its branch
  `release-checks/v<version>`, with `relay-version`, the app version it releases. Given one,
  `relay-production` builds Production's app at that commit, named that version and signed with
  the Production key, and `build` packs it beside Dev's. Empty, as for a release without app
  changes, `build` packs the Production APK GitHub's latest release carries, byte for byte,
  checked against its entry and GitHub's digest of it (`relay.py carry`); none before any release
  carried one. The release stamps that run's package, which Production installs.
- **A PR:** only the debug builds (`android`), unsigned, and no app packed.

`dispatchdev build` packs the builds of the folder `DISPATCH_RELAY_APPS` names, each entry
(`dev.json`, `production.json`) beside its APK, checking each APK against its entry, in the
runtime's `dashboard/relay/` with `relay.json`, the entries together. Without the variable, no app
is packed, as for `npm run dev`. Each server hands out its own build, the Dev build on Dev, at
every DSP's address ([Communications](../features/communications/README.md#dispatch-phones)):
Production's only from a release's package, which alone carries it.

```json
{
  "dev": {
    "version": "0.0.2-dev.1372",
    "versionCode": 1372,
    "commit": "<40 hex>",
    "asset": "dsp-dispatch-relay-0.0.2-dev.1372.apk",
    "sha256": "<64 hex>",
    "size": 18350112
  },
  "production": {
    "version": "0.0.1",
    "versionCode": 1369,
    "commit": "<40 hex>",
    "asset": "dsp-dispatch-relay-0.0.1.apk",
    "sha256": "<64 hex>",
    "size": 18350112
  }
}
```

- `version` and `versionCode` as [above](#versions); `commit`: the commit it was built from.
- `asset`: its APK's file name, `dsp-dispatch-relay-<version>.apk`; `sha256`: its SHA-256 in
  lowercase hex; `size`: its size in bytes, at most 200 MiB.
- `production` only in a release's package, once an app was released.

The format's fixture is `tooling/cli/src/build/relay/relay.json`.

### What the release command does

The release command (`ops/host-manager/src/release/`, the app's part in `relay.rs`) carries the
app into each release:

- **The last released app:** the `production` entry of the `relay.json` attached to GitHub's
  latest release (`releases/latest`), which Production installs and `relay.py` reads, whatever
  the highest version is; without one, none was released.
- **Its version:** the app changed when `git diff <production.commit> <commit> -- android` isn't
  empty, or none was released and `android/` exists. Then `relay-version` is the last released
  app version's next patch (or more, with the release's `--bump`), `0.0.1` first; otherwise it's
  empty and the release carries the last released app. It's decided once for the release's
  commit and kept in the release's journal, so a rerun asks for the same, and refused at or
  below the last released version. Just before publishing, it's checked again against the last
  released app, which a release published meanwhile may have changed: a new version must still
  be above it, and a carried app still exactly it.
- **Its checks:** every release runs
  `gh workflow run checks.yml --ref release-checks/v<version> -f relay-version=<X.Y.Z or empty>`
  and stamps that run's package, never `main`'s push run's, which carries no Production app. Once
  `main`'s push run of the commit passed, it adds `-f tested-run=<that run's id>`: the release's
  run skips the suites that tested the commit's source there, and still builds the apps, packs,
  checks and smoke-tests the package. The package's `production` entry must be that version,
  built at the release's commit, or when carrying, exactly the last release's entry, with its APK
  as the entry names it.
- **Dev:** the release needs Dev serving its commit, by the commit Dev's health reports, not the
  package's digest: Dev runs `main`'s build of the commit, which Production's app makes unlike
  the release's, and its own Dev app may be built again in the release's run.
- **The record:** once an app exists, each published release attaches its package's
  `dashboard/relay/relay.json` and Production's APK, its `asset`, which the next release's run
  (`relay.py carry`) and `main`'s Dev builds (`relay.py version`) read. Both are checked against
  the runtime's manifest, outside `SHA256SUMS`, which keeps to the runtime Production's updater
  installs.
- **Release notes:** generated notes start with the app's version when the release carries a new
  one.

## Updates

Each outbox wait tells the app the version its DSP hands out, and one above its own updates the
app on its own, showing nothing, once Android lets it install (Set up's Install updates): it
downloads it from the DSP, checks its size and SHA-256 against the DSP's details and that it's the
same app signed with the same key, waits until no text is mid-send, and installs it with Android's
`PackageInstaller`. On Android 12+ it asks to install with no one confirming, which Android allows
an app updating itself (`UPDATE_PACKAGES_WITHOUT_USER_ACTION`); Android decides each time. Wherever
it asks anyway, as before Android 12 or under its own rules, a notification asks the person to
update, which opens the app to Android's confirmation, shown at once while the app is open; it goes
once the update installs or the DSP no longer hands it out. A try that doesn't install is tried
again an hour later, or at once from the notification; one at an APK that isn't this app, only
once the DSP hands out another version. The relay starts again once it's replaced.
