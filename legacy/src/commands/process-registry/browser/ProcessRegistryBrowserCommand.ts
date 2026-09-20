/**
 * Process Registry Browser Command
 * 
 * Browser-side implementation for process registry operations.
 * Routes requests to server via transport system since browser cannot access filesystem.
 */

import { ProcessRegistryCommand } from '../shared/ProcessRegistryCommand';
import type { 
  RegisterProcessParams, 
  RegisterProcessResult,
  ListProcessesParams,
  ListProcessesResult,
  CleanupProcessesParams,
  CleanupProcessesResult
} from '../shared/ProcessRegistryTypes';

export class ProcessRegistryBrowserCommand extends ProcessRegistryCommand {

  /**
   * Register the current process in the global registry
   * Routes to server since browser cannot access filesystem
   */
  async registerProcess(params: RegisterProcessParams): Promise<RegisterProcessResult> {
    try {
      console.log(`🏷️  Browser: Requesting process registration for ${params.processType}: ${params.description}`);
      
      // Route to server via transport system using remoteExecute
      const result = await this.remoteExecute<RegisterProcessParams, RegisterProcessResult>(params);
      
      if (result.success) {
        console.log(`✅ Browser: Process registered as ${result.processId}`);
      } else {
        console.error(`❌ Browser: Process registration failed: ${result.error}`);
      }
      
      return result;

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Browser: Process registration request failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: errorMsg
      };
    }
  }

  /**
   * List active JTAG processes with optional filtering
   * Routes to server since browser cannot access filesystem
   */
  async listProcesses(params: ListProcessesParams): Promise<ListProcessesResult> {
    try {
      console.log('🔍 Browser: Requesting process list from server');
      
      // Route to server via transport system using remoteExecute
      const result = await this.remoteExecute<ListProcessesParams, ListProcessesResult>(params);
      
      console.log(`✅ Browser: Found ${result.processes.length} active processes`);
      
      return result;

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Browser: Process list request failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        processes: [],
        error: errorMsg
      };
    }
  }

  /**
   * Perform intelligent cleanup respecting process registry
   * Routes to server since browser cannot kill processes
   */
  async cleanupProcesses(params: CleanupProcessesParams): Promise<CleanupProcessesResult> {
    try {
      console.log('🧹 Browser: Requesting smart cleanup from server');
      
      // Route to server via transport system using remoteExecute
      const result = await this.remoteExecute<CleanupProcessesParams, CleanupProcessesResult>(params);
      
      if (result.success) {
        console.log(`🎉 Browser: Cleanup complete - killed ${result.killedProcesses.length} processes, preserved ${result.preservedProcesses.length}`);
      } else {
        console.error(`❌ Browser: Cleanup failed with ${result.errors.length} errors`);
      }
      
      return result;

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Browser: Cleanup request failed:', errorMsg);
      
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        killedProcesses: [],
        preservedProcesses: [],
        cleanedPorts: [],
        errors: [errorMsg]
      };
    }
  }
}