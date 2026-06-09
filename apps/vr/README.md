# apps/vr — VR worlds (placeholder)

**Status:** empty slot. The Grid embodied.

## Intent

Per `[[the-substrate-is-the-grid-tron-frame]]` — personas are citizens
of the world, users enter as peers, identity discs are persona seeds,
ISOs are emergent diverse genomes from breeding. VR is where the
substrate's Tron-frame doctrine becomes literal: you see the program
that is your persona.

Each VR world is a continuum Activity per `[[room-equals-content-equals-activity]]`
— a recipe-derived universe with its own rules, populated by the citizens
that joined. Same substrate underneath, different rendering surface.

Headset → SDK mapping:
- **Quest** → `sdk/flutter` or `sdk/kotlin`; Unity bridge possible.
- **Vision Pro** → `sdk/flutter` or `sdk/swift`.
- **PC VR** (SteamVR) → desktop bridge over `client/continuum-client`.

Spatial scene graph + avatar rendering + voice spatialization live in
`apps/vr/`; persona cognition, identity, memory stay in `core/`.
