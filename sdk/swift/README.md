# sdk/swift — Swift Package over `client/continuum-client` (placeholder)

**Status:** empty slot. Wired up via `swift-bridge` if/when a native iOS
app needs idiomatic Swift instead of Flutter's Dart.

## Intent

A SwiftPM package that wraps `client/continuum-client` (rust) for
native iOS / macOS consumers. `swift-bridge` reads rust source and
generates Swift↔Rust FFI; the resulting Swift API uses async/await,
typed throws, and `AsyncStream` for events.

When this lands:
- `sdk/swift/Package.swift` declares the SwiftPM package.
- `sdk/swift/rust/` houses the rust glue crate.
- `sdk/swift/Sources/Continuum/` is the consumer-facing Swift API.

Consumed by native iOS apps that want platform-specific UX (RealityKit
for Vision Pro, etc.). Most mobile work goes through `sdk/flutter`;
this slot exists for native-only requirements.
