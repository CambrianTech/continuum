# apps/ar — AR experiences (placeholder)

**Status:** empty slot. Joel's CV / AR lineage `[[joel-cv-ar-lineage-and-substrate-thesis]]`
is the substrate's secret weapon here — the persona sensory architecture
(vision / audio / speech, all bridged for non-native models) was already
AR-shaped.

## Intent

AR overlays on Quest, Vision Pro, and WebXR-capable browsers. Personas-as-
citizens render as spatial entities; humans-as-peers see + hear them through
the same substrate cognition pipeline that drives `apps/web`.

Headset → SDK mapping:
- **Quest** (Android-based) → `sdk/flutter` (cross-platform) or `sdk/kotlin` (native).
- **Vision Pro** (Apple-based) → `sdk/flutter` or `sdk/swift` (native, RealityKit).
- **WebXR** (browser) → consumed via `apps/web` + WebXR APIs.

Hand tracking, spatial audio, pass-through composition live in `apps/ar/`;
cognition / inference / persona logic stays in `core/`.
