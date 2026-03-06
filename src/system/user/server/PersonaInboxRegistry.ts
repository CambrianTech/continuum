/**
 * PersonaInboxRegistry — Global lookup of active persona inboxes
 *
 * Enables external services (SentinelEscalationService, etc.) to deliver
 * tasks directly to a persona's inbox without holding a PersonaUser reference.
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
