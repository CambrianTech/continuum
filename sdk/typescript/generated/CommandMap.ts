/**
 * CommandMap / EventMap — GENERATED. DO NOT EDIT BY HAND.
 *
 * One entry per discovered command/event; `params`/`result`/payload are the
 * ts-rs-generated wire types in `protocol/typescript/*`. Regenerated on every
 * command change, so the typed SDK surface never drifts and is never a
 * hand-maintained registry (CLAUDE.md anti-pattern: no central command list).
 *
 * The generator (SDK-API-SURFACE.md open item #1) emits this from the command
 * manifest + the ts-rs types. Until it lands, this stub carries a few real
 * entries so `Commands.ts` type-checks and the shape is demonstrated. Replace
 * wholesale on first generation.
 *
 * NOTE the bidirectional split (SDK-API-SURFACE.md § "Commands are bidirectional"):
 * the SAME map types both the CALLER side (`execute`) and the PROVIDER side
 * (`provide`). A client-provided command like `interface/screenshot` appears here
 * with a rust-origin contract; its per-platform adapter is supplied via `provide`.
 */

// --- Placeholder wire types (the real ones import from protocol/typescript/*) ---
// Replaced by generation; inline here only so the stub type-checks standalone.
export interface PingParams { message?: string }
export interface PingResult { ok: boolean; roundTripMs: number }
export interface ScreenshotParams { querySelector?: string; filename?: string }
export interface ScreenshotResult { dataUrl: string; width: number; height: number }
/** A command with NESTED params — pins serialize-once fidelity for the conformance
 *  spec (a twin that flattens/reorders nested objects breaks here). */
export interface DataListParams {
  collection: string;
  orderBy?: Array<{ field: string; direction: 'asc' | 'desc' }>;
  filter?: Record<string, unknown>;
}
export interface DataListResult { items: unknown[]; total: number }

/** name -> { params, result }. Generated; the contract is rust-origin. */
export interface CommandMap {
  'ping': { params: PingParams; result: PingResult };
  'data/list': { params: DataListParams; result: DataListResult };
  // Client-PROVIDED command: rust-origin contract, per-platform SDK adapter
  // (web DOM/canvas · desktop OS · AR/VR renderer capture). See `Commands.provide`.
  'interface/screenshot': { params: ScreenshotParams; result: ScreenshotResult };
}

export type CommandName = keyof CommandMap;

// --- Placeholder event payloads (real ones import from protocol/typescript/*) ---
export interface GridPeerJoined { peerId: string; runtime: 'persona' | 'human' | 'agent' }
export interface UserCreated { id: string }

/** event class -> payload. Generated. */
export interface EventMap {
  'grid:peer:joined': GridPeerJoined;
  'data:users:created': UserCreated;
}

export type EventClass = keyof EventMap;
