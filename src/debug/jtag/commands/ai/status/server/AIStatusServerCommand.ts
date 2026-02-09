/**
 * AI Status Server Command
 *
 * Get comprehensive health status of all AI personas in the system
 */

import { AIStatusCommand } from '../shared/AIStatusCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIStatusParams, AIStatusResult, PersonaHealth } from '../shared/AIStatusTypes';
import { UserDaemonServer } from '../../../../daemons/user-daemon/server/UserDaemonServer';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { PersonaUser } from '../../../../system/user/server/PersonaUser';
import { getThoughtStreamCoordinator } from '../../../../system/conversation/server/ThoughtStreamCoordinator';
import { JTAGSystemServer } from '../../../../system/core/system/server/JTAGSystemServer';
import { PersonaToolRegistry } from '../../../../system/user/server/modules/PersonaToolRegistry';

export class AIStatusServerCommand extends AIStatusCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/status', context, subpath, commander);
  }

  async execute(params: AIStatusParams): Promise<AIStatusResult> {
    // Get UserDaemon from systemDaemons (same approach as ping command)
    const sys = JTAGSystemServer.instance;

    if (!sys) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: 'JTAGSystemServer not available',
        summary: { total: 0, healthy: 0, starting: 0, degraded: 0, dead: 0 },
        personas: []
      };
    }

    interface IDaemon {
      name: string;
    }

    const daemons = (sys.systemDaemons ?? []) as IDaemon[];
    const userDaemon = daemons.find(d => d.name === 'UserDaemon') as UserDaemonServer | undefined;

    if (!userDaemon) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: `UserDaemon not found in systemDaemons. Found: ${daemons.map(d => d.name).join(', ')}`,
        summary: { total: 0, healthy: 0, starting: 0, degraded: 0, dead: 0 },
        personas: []
      };
    }

    return this.executeWithDaemon(userDaemon, params);
  }

  private async executeWithDaemon(userDaemon: UserDaemonServer, params: AIStatusParams): Promise<AIStatusResult> {

    // Query all PersonaUser entities from database
    const result = await ORM.query<UserEntity>({
      collection: COLLECTIONS.USERS,
      filter: { type: 'persona' }
    });

    if (!result.success || !result.data) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: 'Failed to query persona users',
        summary: { total: 0, healthy: 0, starting: 0, degraded: 0, dead: 0 },
        personas: []
      };
    }

    // Extract PersonaUser entities
    const personaEntities = result.data.map(r => ({
      ...r.data,
      id: r.id
    } as UserEntity));

    // Filter by name/id if specified
    const filteredPersonas = personaEntities.filter(entity => {
      if (params.personaId && entity.id !== params.personaId) {
        return false;
      }
      if (params.personaName && entity.displayName !== params.personaName) {
        return false;
      }
      return true;
    });

    // Build health info for each persona
    const personaHealthList: PersonaHealth[] = [];
    const summary = { total: 0, healthy: 0, starting: 0, degraded: 0, dead: 0 };

    for (const entity of filteredPersonas) {
      // Get PersonaUser instance from UserDaemon (accessing protected property via type assertion)
      interface UserDaemonWithClients {
        personaClients: Map<string, PersonaUser>;
      }
      const userDaemonWithClients = userDaemon as unknown as UserDaemonWithClients;
      const personaUser = userDaemonWithClients.personaClients.get(entity.id);

      // Extract health metrics (PersonaUser properties are private, need interface)
      interface PersonaUserHealth {
        isInitialized: boolean;
        eventsSubscribed: boolean;
        worker: unknown | null;
        modelConfig: {
          provider: string;
          model: string;
          temperature?: number;
        };
      }

      const personaHealth = personaUser as unknown as PersonaUserHealth | undefined;
      const isInitialized = personaHealth?.isInitialized ?? false;
      const isSubscribed = personaHealth?.eventsSubscribed ?? false;
      const hasWorker = (personaHealth && personaHealth.worker !== null) ? true : false;
      const modelConfig = personaHealth?.modelConfig ?? null;

      // Classify health status
      const status = this.classifyHealthStatus(
        isInitialized,
        isSubscribed,
        hasWorker,
        0, // errorCount - not tracked yet
        undefined // timeSinceLastResponse - not tracked yet
      );

      // Get tools available to this persona
      const toolRegistry = PersonaToolRegistry.sharedInstance();
      const availableTools = toolRegistry.listToolsForPersona(entity.id);

      // Build health object
      const health: PersonaHealth = {
        userId: entity.id,
        displayName: entity.displayName,
        uniqueId: entity.uniqueId,
        status,
        isInitialized,
        isSubscribed,
        hasWorker,
        provider: modelConfig?.provider ?? 'unknown',
        model: modelConfig?.model ?? 'unknown',
        temperature: modelConfig?.temperature,
        toolsAvailable: availableTools.length
      };

      // Add verbose metrics if requested
      if (params.verbose) {
        health.lastResponseTime = undefined; // TODO: Track this
        health.timeSinceLastResponse = undefined;
        health.totalResponses = undefined;
        health.recentResponseRate = undefined;
        health.workerState = hasWorker ? 'running' : 'terminated';
        health.workerUptime = undefined; // TODO: Track this
        health.errorCount = 0; // TODO: Track this
        health.lastError = undefined;
        health.lastErrorTime = undefined;
      }

      // Update summary counts
      summary.total++;
      summary[status]++;

      personaHealthList.push(health);
    }

    // Filter out inactive personas if not requested
    const finalPersonas = params.includeInactive
      ? personaHealthList
      : personaHealthList.filter(p => p.status !== 'dead');

    // Get system metrics if verbose
    let systemMetrics;
    if (params.verbose) {
      const coordinator = getThoughtStreamCoordinator();
      const coordinationStats = coordinator.getCoordinationStats();

      systemMetrics = {
        totalActiveWorkers: personaHealthList.filter(p => p.hasWorker).length,
        totalMemoryUsage: 0, // TODO: Calculate from worker threads
        avgResponseLatency: 0, // TODO: Track this
        thoughtStreamActive: coordinationStats.activeStreams > 0,
        thoughtStreamRejections: coordinationStats.totalRejections
      };
    }

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      summary,
      personas: finalPersonas,
      systemMetrics
    };
  }
}
