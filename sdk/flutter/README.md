# sdk/flutter — Dart SDK over `client/continuum-client` (placeholder)

**Status:** empty slot. Wired up via `flutter_rust_bridge` in Stage 3.

## Intent

Dart package that wraps `client/continuum-client` (rust) for Flutter
consumers. `flutter_rust_bridge` reads rust source and generates the
Dart↔Rust FFI glue; the resulting Dart API mirrors the rust client's
`Connection / CommandClient / EventSubscriber` primitives but with
Dart idioms (`Stream<T>`, `async` / `await`).

When this lands:
- `sdk/flutter/pubspec.yaml` declares the pub.dev package.
- `sdk/flutter/rust/Cargo.toml` is a thin rust crate that re-exports
  `client/continuum-client` + adds `#[frb]` annotations for codegen.
- `sdk/flutter/lib/continuum.dart` is the consumer-facing Dart API.

Consumed by `apps/mobile`, `apps/ar`, `apps/vr` (any platform Flutter
can build for).
