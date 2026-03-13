/**
 * AI Status Command Types
 *
 * Get comprehensive status of all AI personas in the system
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

export interface AIStatusParams extends CommandParams {
  // Filtering
  personaId?: UUID;                    // Check specific persona
  personaName?: string;                // Check persona by display name
  includeInactive?: boolean;           // Include personas not responding (default: false)

  // Output control
  verbose?: boolean;                   // Include detailed health metrics (default: false)
}

export interface PersonaHealth {
  userId: UUID;
  displayName: string;
  uniqueId: string;

  // Health status
  status: 'healthy' | 'starting' | 'degraded' | 'dead';

  // Basic metrics
  isInitialized: boolean;
  isSubscribed: boolean;
  hasWorker: boolean;

  // Response metrics (if verbose)
  lastResponseTime?: number;           // Unix timestamp of last response
  timeSinceLastResponse?: number;      // Milliseconds since last response
  totalResponses?: number;             // Lifetime response count
  recentResponseRate?: number;         // Responses in last hour

  // Model configuration
  provider: string;                    // 'anthropic' | 'candle' | 'openai' etc.
  model: string;                       // 'claude-3-5-sonnet-20241022' | 'llama3.2:3b' etc.
  temperature?: number;

  // Worker thread health (if verbose)
  workerState?: 'running' | 'idle' | 'error' | 'terminated';
  workerUptime?: number;               // Milliseconds since worker started

  // Error tracking
  errorCount?: number;                 // Total errors in session
  lastError?: string;                  // Most recent error message
  lastErrorTime?: number;              // Unix timestamp of last error

  // Tool capabilities
  toolsAvailable?: number;             // Number of tools available to this persona
}

export interface AIStatusResult extends CommandResult {
  success: boolean;
  error?: string;

  // Summary counts
  summary: {
    total: number;
    healthy: number;
    starting: number;
    degraded: number;
    dead: number;
  };

  // Persona details
  personas: PersonaHealth[];

  // System-wide metrics (if verbose)
  systemMetrics?: {
    totalActiveWorkers: number;
    totalMemoryUsage: number;          // Bytes (if available)
    avgResponseLatency: number;        // Milliseconds
    thoughtStreamActive: boolean;
    thoughtStreamRejections: number;
  };
}

/**
 * AIStatus — Type-safe command executor
 *
 * Usage:
 *   import { AIStatus } from '...shared/AIStatusTypes';
 *   const result = await AIStatus.execute({ ... });
 */
export const AIStatus = {
  execute(params: CommandInput<AIStatusParams>): Promise<AIStatusResult> {
    return Commands.execute<AIStatusParams, AIStatusResult>('ai/status', params as Partial<AIStatusParams>);
  },
  commandName: 'ai/status' as const,
} as const;

/**
 * Factory function for creating AiStatusParams
 */
export const createAIStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIStatusParams, 'context' | 'sessionId' | 'userId'>
): AIStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiStatusResult with defaults
 */
export const createAIStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIStatusResult, 'context' | 'sessionId' | 'userId'>
): AIStatusResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAIStatusResultFromParams = (
  params: AIStatusParams,
  differences: Omit<AIStatusResult, 'context' | 'sessionId' | 'userId'>
): AIStatusResult => transformPayload(params, differences);

