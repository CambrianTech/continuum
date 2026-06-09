# sdk/kotlin — Android AAR over `client/continuum-client` (placeholder)

**Status:** empty slot. Wired up via `uniffi` if/when a native Android
app needs idiomatic Kotlin instead of Flutter's Dart.

## Intent

An Android `.aar` artifact that wraps `client/continuum-client` (rust)
for native Kotlin consumers. `uniffi` (Mozilla's cross-language bindings
generator) reads rust source and emits Kotlin bindings; the resulting
Kotlin API uses suspend functions, sealed classes, and `Flow<T>` for
events.

When this lands:
- `sdk/kotlin/build.gradle.kts` declares the AAR build.
- `sdk/kotlin/rust/` houses the rust glue crate (uniffi UDL definitions).
- `sdk/kotlin/src/main/kotlin/com/continuum/` is the consumer-facing
  Kotlin API.

Consumed by native Android apps (Quest VR + traditional phones).
Most cross-platform mobile work goes through `sdk/flutter`; this slot
exists for native-only requirements.
