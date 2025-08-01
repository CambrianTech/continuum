/**
 * Ping Command - Shared Base
 * 
 * Base implementation for ping command that provides timing and environment detection.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type PingParams, type PingResult, createPingResultFromParams } from './PingTypes';

export abstract class PingCommand extends CommandBase<PingParams, PingResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ping', context, subpath, commander);
  }

  /**
   * Execute ping command with timing measurement
   */
  async execute(params: JTAGPayload): Promise<PingResult> {
    const startTime = Date.now();
    const pingParams = params as PingParams;
    
    console.log(`🏓 ${this.getEnvironmentLabel()}: Ping received - "${pingParams.message || 'ping'}"`);

    try {
      // Get environment-specific information
      const environment = await this.getEnvironmentInfo();
      
      // Calculate round-trip time (simulate some processing)
      await new Promise(resolve => setTimeout(resolve, 1));
      const roundTripTime = pingParams.includeTiming ? Date.now() - startTime : undefined;
      
      console.log(`✅ ${this.getEnvironmentLabel()}: Pong! Round-trip: ${roundTripTime}ms`);

      return createPingResultFromParams(pingParams, {
        success: true,
        roundTripTime,
        environment: pingParams.includeEnvironment ? environment : undefined
      });

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Ping failed:`, error);
      
      return createPingResultFromParams(pingParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown ping error'
      });
    }
  }

  /**
   * Get environment-specific information
   */
  protected abstract getEnvironmentInfo(): Promise<PingResult['environment']>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}