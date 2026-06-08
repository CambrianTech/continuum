# sdk/ — per-language SDKs over `client/continuum-client`

Language wrappers that bridge `client/continuum-client` (rust) to the
native ecosystem of each platform. CBAR-style: one shared core, N
language frontends, mirrored API surface.

| SDK | Bridge | Consumed by |
|-----|--------|-------------|
| `typescript/` | hand-written (today); flutter_rust_bridge alt later | `apps/web`, `apps/desktop` (Tauri), `apps/mcp` (Node MCP server) |
| `flutter/` | `flutter_rust_bridge` — Dart over rust FFI | `apps/mobile`, `apps/ar`, `apps/vr` |
| `swift/` | `swift-bridge` — Swift package | native iOS apps (when Flutter isn't enough) |
| `kotlin/` | `uniffi` — `.aar` artifact | native Android apps |

Each SDK's job is to give the platform's developers an idiomatic API
shape (Dart streams, Swift async/await, Kotlin coroutines) over the
same `Connection / CommandClient / EventSubscriber` primitives the rust
client exposes. The substrate stays oblivious to language.
