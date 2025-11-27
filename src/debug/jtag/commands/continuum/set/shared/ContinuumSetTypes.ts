/**
 * Continuum Set Command Types
 *
 * Universal control of the Continuum widget - anyone (human via CLI,
 * PersonaUser, external AI, tests) can temporarily control the Continuum
 * widget to display custom status.
 *
 * The Continuum widget is the shared emotional interface between humans
 * and AIs - inspired by HAL 9000 and Tron aesthetics.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface ContinuumSetParams extends CommandParams {
  /** Emoji to display in/near the Continuum dot */
  emoji?: string;

  /** CSS color value for the dot (e.g., 'blue', '#00ff00', 'rgb(255, 0, 0)') */
  color?: string;

  /** Text message to display under the Continuum dot */
  message?: string;

  /**
   * Auto-revert duration in milliseconds
   * After this duration, Continuum returns to system status
   * Default: 5000 (5 seconds)
   */
  duration?: number;

  /**
   * Immediately clear and return to system status
   * When true, all other parameters are ignored
   */
  clear?: boolean;

  /**
   * Priority level for status display
   * Higher priority statuses can override lower priority ones
   * 'low' | 'medium' | 'high' | 'critical'
   * Default: 'medium'
   */
  priority?: 'low' | 'medium' | 'high' | 'critical';
}

export interface ContinuumSetResult extends CommandResult {
  success: boolean;
  message: string;

  /** Current Continuum status after update */
  status: {
    emoji?: string;
    color?: string;
    message?: string;
    source: string;  // Who set this status ('cli', 'persona', 'system', etc.)
    priority: 'low' | 'medium' | 'high' | 'critical';
    timestamp: number;
    autoRevertAt?: number;  // Timestamp when this will revert
  };
}
