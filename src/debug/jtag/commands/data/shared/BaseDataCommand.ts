/**
 * Base Data Command - Generic Backend Routing
 *
 * Following ARCHITECTURE-RULES.md:
 * ✅ Generic programming - works with ANY entity extending BaseEntity
 * ✅ Environment routing abstraction (server/browser)
 * ✅ LocalStorage adapter integration
 * ✅ No specific entity references
 */

import { CommandBase } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { BaseDataParams, BaseDataResult } from './BaseDataTypes';

/**
 * Abstract base for all data commands
 * Handles backend routing logic generically
 */
export abstract class BaseDataCommand<
  P extends BaseDataParams = BaseDataParams,
  R extends BaseDataResult = BaseDataResult
> extends CommandBase<P, R> {

  constructor(commandName: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(commandName, context, subpath, commander);
  }

  /**
   * Generic execute with environment routing
   */
  async execute(params: P): Promise<R> {
    // Ensure backend is set (defensive programming)
    const effectiveParams = {
      ...params,
      backend: params.backend ?? 'server'
    } as P;

    console.log(`🔧 BaseDataCommand.execute():`, {
      commandName: this.name,
      requestedBackend: effectiveParams.backend,
      currentEnvironment: this.context.environment,
      willDelegate: this.context.environment !== effectiveParams.backend
    });

    // If we're not in the requested environment, delegate
    if (this.context.environment !== effectiveParams.backend) {
      console.log(`🔧 BaseDataCommand: Delegating to ${effectiveParams.backend} environment`);
      return await this.remoteExecute(effectiveParams);
    }

    // We're in the correct environment, execute locally
    console.log(`🔧 BaseDataCommand: Executing locally in ${this.context.environment} environment`);
    return await this.executeDataCommand(effectiveParams);
  }

  /**
   * Abstract method for data operation implementation
   * Subclasses implement this for their specific operation
   */
  protected abstract executeDataCommand(params: P): Promise<R>;
}