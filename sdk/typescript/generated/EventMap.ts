/**
 * EventMap — the typed event surface.
 *
 * HAND-MAINTAINED for now: unlike CommandMap (Rust-generated from each command's
 * CommandSpec), events are not yet declared as Rust EventSpecs, so this file is
 * the interim source. When events become Rust-rooted (an EventSpec registry the
 * generator walks, mirroring commands), this file becomes generated too and the
 * payload types move to vendored `wire/*`. Until then, keep entries here.
 *
 * Split out of the generated CommandMap so the command surface can be regenerated
 * wholesale (sdk_codegen) without clobbering events.
 */

// --- Placeholder event payloads (real ones will vendor from wire/* once Rust-sourced) ---
export interface GridPeerJoined { peerId: string; runtime: 'persona' | 'human' | 'agent' }
export interface UserCreated { id: string }

/** event class -> payload. */
export interface EventMap {
  'grid:peer:joined': GridPeerJoined;
  'data:users:created': UserCreated;
}

export type EventClass = keyof EventMap;
