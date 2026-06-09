/**
 * PersonaInboxRegistry — Global lookup of active persona inboxes
 *
 * Enables TS-side services to deliver tasks directly to a persona's
 * inbox without holding a PersonaUser reference. Sentinel-completion
 * deliveries no longer go through this registry — Rust's
 * `core/continuum-core/src/modules/sentinel/escalation.rs` writes
 * tasks via `data/create` so the channel tick (also in Rust) picks
 * them up.
 *
 * PersonaUser registers on init, unregisters on shutdown.
 */

import type { PersonaInbox } from './modules/PersonaInbox';
import type { UUID } from '../../core/types/CrossPlatformUUID';

const registry = new Map<UUID, PersonaInbox>();

/** Register a persona's inbox (called by PersonaUser on init) */
export function registerPersonaInbox(personaId: UUID, inbox: PersonaInbox): void {
  registry.set(personaId, inbox);
}

/** Unregister a persona's inbox (called by PersonaUser on shutdown) */
export function unregisterPersonaInbox(personaId: UUID): void {
  registry.delete(personaId);
}

/** Get a persona's inbox by ID (returns null if not active) */
export function getPersonaInbox(personaId: UUID): PersonaInbox | null {
  return registry.get(personaId) ?? null;
}
