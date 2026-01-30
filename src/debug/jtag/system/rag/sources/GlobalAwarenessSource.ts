/**
 * GlobalAwarenessSource - Injects cross-context awareness into RAG via Rust IPC
 *
 * Delegates to Rust's consciousness context builder which runs:
 * - Temporal continuity queries (what was I doing before?)
 * - Cross-context event aggregation (what happened in other rooms?)
 * - Active intention detection (interrupted tasks)
 * - Peripheral activity check
 *
 * All queries run concurrently in Rust with separate SQLite read connections,
 * bypassing the Node.js event loop entirely.
 *
 * Previous implementation used TS UnifiedConsciousness through the event loop
 * (3-60s under load, frequently timing out).
 *
 * Priority 85 - After identity (95), before conversation history (80).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('GlobalAwarenessSource', 'rag');

/**
 * Registry for consciousness instances — kept for backward compatibility.
 * The actual consciousness context is now built in Rust via IPC,
 * but we still need the registry to check if a persona has been initialized.
 */
const initializedPersonas = new Set<string>();

/**
 * Register a persona as having consciousness initialized.
 * Called during PersonaUser startup after memory/init succeeds.
 */
export function registerConsciousness(personaId: string, _consciousness?: any): void {
  initializedPersonas.add(personaId);
  log.debug(`Registered consciousness for persona ${personaId}`);
}

/**
 * Unregister a persona's consciousness
 */
export function unregisterConsciousness(personaId: string): void {
  initializedPersonas.delete(personaId);
  log.debug(`Unregistered consciousness for persona ${personaId}`);
}

/**
 * Check if a persona has consciousness registered
 */
export function getConsciousness(personaId: string): boolean {
  return initializedPersonas.has(personaId);
}

export class GlobalAwarenessSource implements RAGSource {
  readonly name = 'global-awareness';
  readonly priority = 85;  // After identity (95), before conversation (80)
  readonly defaultBudgetPercent = 10;

  isApplicable(context: RAGSourceContext): boolean {
    return initializedPersonas.has(context.personaId);
  }

  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      // Get PersonaUser to access Rust bridge
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        log.debug('UserDaemon not available, skipping awareness');
        return this.emptySection(startTime);
      }

      const personaUser = userDaemon.getPersonaUser(context.personaId);
      if (!personaUser) {
        return this.emptySection(startTime);
      }

      // Access the Rust cognition bridge (nullable getter — no throw)
      const bridge = (personaUser as any).rustCognitionBridge;
      if (!bridge) {
        log.debug('Rust cognition bridge not available, skipping awareness');
        return this.emptySection(startTime);
      }

      // Detect voice mode — skip expensive semantic search for faster response
      const voiceSessionId = (context.options as any)?.voiceSessionId;
      const isVoiceMode = !!voiceSessionId;

      const currentMessage = context.options.currentMessage?.content;

      // Single IPC call → Rust builds consciousness context with concurrent SQLite reads
      // Rust handles its own 30s TTL cache internally
      const result = await bridge.memoryConsciousnessContext(
        context.roomId,
        currentMessage,
        isVoiceMode  // skipSemanticSearch
      );

      if (!result.formatted_prompt) {
        log.debug(`No cross-context content for room ${context.roomId}`);
        return this.emptySection(startTime);
      }

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(result.formatted_prompt);

      log.debug(`Loaded global awareness in ${loadTimeMs.toFixed(1)}ms (${tokenCount} tokens) rust=${result.build_time_ms.toFixed(1)}ms`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        systemPromptSection: result.formatted_prompt,
        metadata: {
          crossContextEventCount: result.cross_context_event_count,
          activeIntentionCount: result.active_intention_count,
          hasPeripheralActivity: result.has_peripheral_activity,
          wasInterrupted: result.temporal.was_interrupted,
          lastActiveContext: result.temporal.last_active_context_name,
          rustBuildMs: result.build_time_ms
        }
      };

    } catch (error: any) {
      log.error(`Failed to load global awareness: ${error.message}`);
      return this.errorSection(startTime, error.message);
    }
  }

  private emptySection(startTime: number): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { empty: true }
    };
  }

  private errorSection(startTime: number, error: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { error }
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
