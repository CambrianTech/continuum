/**
 * Server-side artifacts daemon check command
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ArtifactsCheckParams, ArtifactsCheckResult } from '../shared/ArtifactsCheckTypes';
import type { JTAGContext, JTAGMessage } from '../../../../system/core/types/JTAGTypes';
import type { ArtifactsResponse, ArtifactsPayload } from '../../../../daemons/artifacts-daemon/shared/ArtifactsDaemon';

export class ArtifactsCheckServerCommand extends CommandBase<ArtifactsCheckParams, ArtifactsCheckResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('artifacts-check', context, subpath, commander);
  }

  async execute(params: ArtifactsCheckParams): Promise<ArtifactsCheckResult> {
    console.log('🔍 ArtifactsCheck: Starting daemon check...');

    try {
      // Get local system to check daemons
      const { JTAGSystemServer } = await import('../../../../system/core/system/server/JTAGSystemServer');
      const system = JTAGSystemServer.instance;

      if (!system) {
        return {
          daemonFound: false,
          systemInfo: {
            totalDaemons: 0,
            daemonList: []
          },
          context: params.context,
          sessionId: params.sessionId
        };
      }

      // Get daemon list from system - use systemDaemons instead of protected daemons
      const daemonList = system.systemDaemons ? system.systemDaemons.map(d => d.name) : [];
      const hasDaemon = daemonList.includes('artifacts-daemon');

      console.log(`🔍 ArtifactsCheck: Found ${daemonList.length} daemons:`, daemonList);
      console.log(`🔍 ArtifactsCheck: ArtifactsDaemon present? ${hasDaemon}`);

      // Try to get the daemon directly from systemDaemons array
      const artifactsDaemon = system.systemDaemons?.find(d => d.name === 'artifacts-daemon');

      let testResult;
      if (artifactsDaemon && params.testFile) {
        console.log(`🔍 ArtifactsCheck: Testing read operation on ${params.testFile}...`);
        try {
          // Test reading a file through the daemon's handleMessage
          const { createArtifactsPayload } = await import('../../../../daemons/artifacts-daemon/shared/ArtifactsDaemon');

          const testPayload: ArtifactsPayload = createArtifactsPayload(params.context, params.sessionId, {
            operation: 'read' as const,
            relativePath: params.testFile,
            storageType: 'system' as const
          });

          const testMessage: JTAGMessage = {
            id: `test-${Date.now()}`,
            type: 'request',
            target: 'artifacts-daemon',
            source: params.context.environment,
            payload: testPayload,
            timestamp: new Date().toISOString(),
            context: params.context,
            sessionId: params.sessionId
          } as unknown as JTAGMessage;

          const response: ArtifactsResponse = await artifactsDaemon.handleMessage(testMessage) as unknown as ArtifactsResponse;

          testResult = {
            operation: 'read',
            success: response.result.success,
            data: response.result.data ? `${String(response.result.data).slice(0, 50)}...` : undefined,
            error: response.result.error
          };

          console.log(`✅ ArtifactsCheck: Daemon message processing ${response.result.success ? 'succeeded' : 'failed'}`);
        } catch (error) {
          const errorMsg = error instanceof Error ? error.message : String(error);
          testResult = {
            operation: 'read',
            success: false,
            error: errorMsg
          };
          console.error(`❌ ArtifactsCheck: Test read failed:`, error);
        }
      }

      return {
        daemonFound: hasDaemon,
        daemonName: hasDaemon ? 'ArtifactsDaemon' : undefined,
        testResult,
        systemInfo: {
          totalDaemons: daemonList.length,
          daemonList
        },
        context: params.context,
        sessionId: params.sessionId
      };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ ArtifactsCheck: Error checking daemon:', error);

      return {
        daemonFound: false,
        systemInfo: {
          totalDaemons: 0,
          daemonList: []
        },
        context: params.context,
        sessionId: params.sessionId
      };
    }
  }
}
