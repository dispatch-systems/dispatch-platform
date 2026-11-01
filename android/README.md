# Android apps

Dispatch's Android apps, in one Gradle build: each app in a folder of its own, and anything they
come to share in one beside them. Developed on the MacBook, with Android Studio's SDK; CI runs
their unit tests, lint and debug builds only when a change touches this folder
(`.github/workflows/android.yml`).

- `relay/`: **DSP Dispatch: Relay**, the app on a DSP's dispatch phone. It relays the phone's
  texts and pictures to the DSP's Dispatch and sends Dispatch's from the phone's number, as
  [the phones' protocol](../features/communications/api/phone/README.md) has it.

## Building

From this folder, with the SDK at `~/Library/Android/sdk` (`ANDROID_HOME`) and Java 21:

```sh
./gradlew test lint assembleDebug   # what CI runs
./gradlew installDevDebug           # the dev app, debug build, on the phone or emulator adb sees
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
