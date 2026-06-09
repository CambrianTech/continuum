# protocol/ — generated wire bindings (do not edit)

Output dir for cross-language type generators that read rust source
annotations (`#[derive(TS)]`, `#[derive(uniffi::Record)]`,
`#[swift_bridge::bridge]`) and emit per-language type bindings.

| Subdir | Producer | Consumer |
|--------|----------|----------|
| `typescript/` | `ts-rs` (cargo test triggers regeneration) | `sdk/typescript`, legacy `src/`, browser bundle |
| `swift/` | `swift-bridge` (when Stage 3 lands swift SDK) | `sdk/swift` |
| `kotlin/` | `uniffi` (when Stage 3 lands kotlin SDK) | `sdk/kotlin` |
| `flutter/` | `flutter_rust_bridge` (when Stage 3 lands flutter SDK) | `sdk/flutter` |

**Don't hand-edit anything here.** These dirs are regenerated from rust
sources. If a generated type looks wrong, fix the `#[derive(...)]`
attribute on the originating rust struct and re-run the generator.
