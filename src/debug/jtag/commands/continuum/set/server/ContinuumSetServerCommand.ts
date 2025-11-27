/**
 * Continuum Set Command - Server Implementation
 *
 * Emits continuum:status event that ContinuumWidget listens to
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ContinuumSetCommand } from '../shared/ContinuumSetCommand';
import type { ContinuumSetParams, ContinuumSetResult } from '../shared/ContinuumSetTypes';
import { Events } from '../../../../system/core/shared/Events';

export class ContinuumSetServerCommand extends ContinuumSetCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeContinuumSet(params: ContinuumSetParams): Promise<ContinuumSetResult> {
    // Default duration to 5000ms (5 seconds)
    const duration = params.duration ?? 5000;
    const priority = params.priority ?? 'medium';

    // Handle clear request
    if (params.clear) {
      console.log('🔄 Continuum: Clearing status, returning to system default');

      // Emit clear event (fire-and-forget)
      // CRITICAL: Pass this.context so event routes to ALL connected clients
      Events.emit(this.context, 'continuum:status', {
        clear: true,
        source: this.getSource(params),
        priority: 'high',  // Clear has high priority
        timestamp: Date.now()
      });

      return transformPayload(params, {
        success: true,
        message: 'Continuum status cleared',
        status: {
          source: 'system',
          priority: 'low',
          timestamp: Date.now()
        }
      });
    }

    // Validate at least one parameter is provided
    if (!params.emoji && !params.color && !params.message) {
      return transformPayload(params, {
        success: false,
        message: 'Must provide at least one of: emoji, color, or message',
        status: {
          source: 'system',
          priority: 'low',
          timestamp: Date.now()
        }
      });
    }

    const timestamp = Date.now();
    const autoRevertAt = duration > 0 ? timestamp + duration : undefined;

    // Create status object
    const status = {
      emoji: params.emoji,
      color: params.color,
      message: params.message,
      source: this.getSource(params),
      priority,
      timestamp,
      autoRevertAt
    };

    console.log(`✨ Continuum: Setting status`, {
      emoji: params.emoji,
      color: params.color,
      message: params.message,
      source: status.source,
      priority,
      duration: duration > 0 ? `${duration}ms` : 'permanent'
    });

    // Emit event to ContinuumWidget (fire-and-forget, auto-bridges to browser)
    // CRITICAL: Pass this.context so event routes to ALL connected clients, not just CLI context
    Events.emit(this.context, 'continuum:status', status);

    // Schedule auto-revert if duration specified
    if (autoRevertAt) {
      setTimeout(() => {
        console.log(`🔄 Continuum: Auto-reverting status after ${duration}ms`);
        Events.emit(this.context, 'continuum:status', {
          clear: true,
          source: 'system',
          priority: 'low',
          timestamp: Date.now()
        });
      }, duration);
    }

    return transformPayload(params, {
      success: true,
      message: `Continuum status set${autoRevertAt ? ` (auto-revert in ${duration}ms)` : ''}`,
      status
    });
  }

  /**
   * Determine source of the status update
   */
  private getSource(params: ContinuumSetParams): string {
    // Check if caller is a PersonaUser
    if (params.context?.callerType === 'persona') {
      return 'persona';
    }

    // Check if caller is human via CLI
    if (params.context?.callerType === 'human') {
      return 'cli';
    }

    // Check if called by script/system
    if (params.context?.callerType === 'script') {
      return 'system';
    }

    // Default to unknown
    return 'unknown';
  }
}
