# Android apps

Dispatch's Android apps, in one Gradle build: each app in a folder of its own, and anything they
come to share in one beside them. Developed on the MacBook, with Android Studio's SDK. CI's
`android` job (`.github/workflows/checks.yml`) runs their unit tests, lint and debug builds when
a change touches this folder, and passes at once for any other.

- `relay/`: **DSP Dispatch: Relay**, the app on a DSP's dispatch phone. It relays the phone's
  texts and pictures to the DSP's Dispatch and sends Dispatch's from the phone's number, as
  [the phones' protocol](../features/communications/api/phone/README.md) has it.

## Building

From this folder, with the SDK at `~/Library/Android/sdk` (`ANDROID_HOME`) and Java 21:

```sh
./gradlew test lintDevDebug lintProductionRelease assembleDebug  # what CI runs
./gradlew installDevDebug  # the dev app, debug build, on the phone or emulator adb sees
```

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

Release builds are signed on the MacBook alone. The key is kept outside the repo, with a copy in
the owner's password manager: a phone only updates to an APK signed with the key it was
installed with. `dispatch.relay.signing` in `~/.gradle/gradle.properties` names a properties file
of `storeFile`, `storePassword`, `keyAlias` and `keyPassword`; without it, a release builds
unsigned.

## Publishing

Each version is published from the MacBook as a GitHub pre-release of this repository, tagged
`relay-v<version>`, with both builds' signed APKs: a pre-release is never GitHub's latest
release, which Production installs. `relay/release.json` names the version published, and
merged, it's what Dispatch hands out:

```json
{
  "version": "1.4.0",
  "versionCode": 14,
  "builds": {
    "dev": {
      "asset": "dsp-dispatch-relay-dev-1.4.0.apk",
      "sha256": "<64 hex>",
      "size": 18350112
    },
    "production": {
      "asset": "dsp-dispatch-relay-1.4.0.apk",
      "sha256": "<64 hex>",
      "size": 18350112
    }
  }
}
```

- `version`: three numbers, as `1.4.0`. `versionCode`: the app's, 1 or more, higher with each
  version, which Android compares to tell a newer one.
- `builds`: `dev` and `production`, each its APK's asset name on the pre-release (letters,
  digits, `.`, `_` and `-`, ending `.apk`, one of its own), its SHA-256 in lowercase hex, and its
  size in bytes.

`dispatchdev build`, which CI's build and `npm run dev` run, fetches each APK from the
pre-release once, into a cache every worktree shares, refuses one whose SHA-256 or size differs,
fetched or kept, and packs both with the file in the runtime's `dashboard/relay/`. A debug build
that can't download one, as offline, warns and packs none; a release build fails. Dev serves the new version as it installs `main`, and Production
once a release includes it. Each server hands out its own build, the Dev build on Dev, at every
DSP's address ([Communications](../features/communications/README.md#dispatch-phones)). Without
the file, no app is packed or offered. The format's fixture is
`tooling/cli/src/build/relay/release.json`.
