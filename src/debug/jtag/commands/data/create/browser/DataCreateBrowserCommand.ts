/**
 * Data Create Command - Browser Implementation
 *
 * Handles localStorage operations locally, delegates server operations
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DataCreateCommand } from '../shared/DataCreateCommand';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';

export class DataCreateBrowserCommand extends DataCreateCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser implementation: handles localStorage backend only
   */
  protected async executeDataCommand(params: DataCreateParams): Promise<DataCreateResult> {
    // Browser only handles browser environment
    // Server environment requests are delegated by base class

    console.log(`🗄️ DataCreateBrowser: Handling localStorage create`, {
      collection: params.collection,
      dataType: typeof params.data,
      hasId: !!params.data?.id
    });

    try {
      console.log(`🗄️ DataCreateBrowser: Calling LocalStorageDataBackend.create()...`);
      const result = await LocalStorageDataBackend.create(params.collection, params.data);
      console.log(`🗄️ DataCreateBrowser: LocalStorageDataBackend.create() returned:`, result);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: result.success,
        data: result.success ? params.data : undefined,
        error: result.error,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      console.error(`❌ DataCreateBrowser: Exception in executeDataCommand:`, error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: String(error),
        timestamp: new Date().toISOString()
      };
    }
  }
}