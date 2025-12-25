/**
 * User Capabilities Defaults - Single Source of Truth
 *
 * ARCHITECTURE-RULES.md compliance:
 * - Single source of truth for user type capabilities
 * - No duplication across PersonaUser/AgentUser/HumanUser
 * - Type-safe with discriminated union pattern
 */

import type { UserCapabilities } from '../../data/entities/UserEntity';

/**
 * User Type Capabilities Configuration
 * Single source of truth for default capabilities by user type
 */
export const USER_CAPABILITIES_DEFAULTS: Record<'human' | 'agent' | 'persona', UserCapabilities> = {
  human: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: true,       // Humans can create rooms
    canInviteOthers: true,      // Humans can invite others
    canModerate: true,          // Humans can moderate
    autoResponds: false,
    providesContext: false,
    canTrain: false,
    canAccessPersonas: true     // Humans can create/manage personas
  },
  agent: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: false,
    canInviteOthers: false,
    canModerate: false,
    autoResponds: false,       // Agents don't auto-respond (controlled externally)
    providesContext: false,
    canTrain: false,
    canAccessPersonas: false
  },
  persona: {
    canSendMessages: true,
    canReceiveMessages: true,
    canCreateRooms: false,
    canInviteOthers: false,
    canModerate: false,
    autoResponds: true,        // Personas respond automatically
    providesContext: true,     // Personas provide RAG context
    canTrain: false,
    canAccessPersonas: false
  }
} as const;

/**
 * User State Preferences Defaults
 * Single source of truth for default preferences by user type
 */
export const USER_PREFERENCES_DEFAULTS: Record<'human' | 'agent' | 'persona', {
  maxOpenTabs: number;
  autoCloseAfterDays: number;
  rememberScrollPosition: boolean;
  syncAcrossDevices: boolean;
}> = {
  human: {
    maxOpenTabs: 100,       // Humans: unlimited practical use
    autoCloseAfterDays: 30, // Long retention
    rememberScrollPosition: true,
    syncAcrossDevices: true // Humans sync across devices
  },
  agent: {
    maxOpenTabs: 50,        // Agents: plenty of tabs
    autoCloseAfterDays: 7,  // Week retention
    rememberScrollPosition: false,
    syncAcrossDevices: false
  },
  persona: {
    maxOpenTabs: 50,        // Personas: plenty of tabs
    autoCloseAfterDays: 7, // Keep state for a week
    rememberScrollPosition: true,
    syncAcrossDevices: false  // Personas don't sync across devices
  }
} as const;

/**
 * Get default capabilities for user type
 */
export function getDefaultCapabilitiesForType(userType: 'human' | 'agent' | 'persona'): UserCapabilities {
  return { ...USER_CAPABILITIES_DEFAULTS[userType] };
}

/**
 * Get default preferences for user type
 */
export function getDefaultPreferencesForType(userType: 'human' | 'agent' | 'persona'): {
  maxOpenTabs: number;
  autoCloseAfterDays: number;
  rememberScrollPosition: boolean;
  syncAcrossDevices: boolean;
} {
  return { ...USER_PREFERENCES_DEFAULTS[userType] };
}
