# apps/mobile — Flutter app for iOS + Android (placeholder)

**Status:** empty slot. Waits on `sdk/flutter` (Stage 3).

## Intent

One Flutter codebase, two embodiments (iOS, Android). Per
`[[citizens-have-envs-not-the-other-way-around]]` — the phone is its
own embodiment of the same citizen identity (the user's airc keypair),
not a "different account."

When this lands:
- `apps/mobile/pubspec.yaml` depends on `sdk/flutter` (Dart pkg over
  `client/continuum-client` via `flutter_rust_bridge`).
- `apps/mobile/lib/main.dart` builds the standard substrate primitives
  (connection / commands / events) into a Flutter widget tree.
- Native iOS / Android quirks (background processing, push, deep links)
  live in `apps/mobile/{ios,android}/` per Flutter convention.

CBAR pattern reference: cb-mobile-sdk's parent C++ → per-platform
Obj-C/Swift + Java/Kotlin shells. Same shape, different tech: substrate
rust → flutter_rust_bridge Dart → one app.
