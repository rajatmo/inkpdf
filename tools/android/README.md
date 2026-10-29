# Android test build

`build-apk.sh` makes a signed arm64 APK of InkPDF (NativeActivity, no Java); `ABIS="x86_64"` builds for an emulator on a PC. It is
signed with `debug.keystore` in this folder (the standard Android debug credentials: store and key password `android`, alias
`androiddebugkey`): **a test key, not a release key**, committed on purpose so that every build installs over the last one. Install with
`adb install -r inkpdf-android-test.apk` (or copy it to the device and allow installs from that app). CI builds the APK as the
`inkpdf-android-test-apk` artifact. Read what the app says with `adb logcat -s inkpdf`.

You need the Rust target (`rustup target add aarch64-linux-android`), the NDK (r27 or newer), the SDK's build-tools and platform
`android.jar`, a JDK and `zip`; the script says which environment variables to set (`ANDROID_NDK_HOME`, `ANDROID_BUILD_TOOLS`,
`ANDROID_JAR`). `emulator-test.sh` runs the APK on an emulator and leaves screenshots and the log (CI runs it too).

What works: pen with pressure, hover and the points between events; an S Pen's one button erases (a second button lassos, a real eraser end
erases); finger pan, pinch and taps on the buttons; palm rejection while the pen is near; the soft keyboard for text and the file name;
Save As. The app starts on a welcome PDF and saves into its own folder, `Android/data/app.inkpdf.test/files/Documents` (no permission
needed); `adb pull` it from there. What does not work yet: opening PDFs from other apps or a file picker, the clipboard, opening links,
system bars and cutouts (see `docs/PORTING.md`, steps A3 and A4).
