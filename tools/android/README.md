# Android test build

`build-apk.sh` makes a signed arm64 APK of InkPDF (NativeActivity, no Java). It is signed with `debug.keystore` in this folder
(the standard Android debug credentials: store and key password `android`, alias `androiddebugkey`): **a test key, not a release
key**, committed on purpose so that every build installs over the last one. Install with `adb install -r inkpdf-android-test.apk`
(or copy it to the device and allow installs from that app). Read what the app says with `adb logcat -s inkpdf`.

The app saves into its own folder, `Android/data/app.inkpdf.test/files/Documents` (no permission needed); `adb pull` it from there.
See `docs/PORTING.md` for what does not work yet (pen pressure, eraser and barrel button, files from other apps, the clipboard).
