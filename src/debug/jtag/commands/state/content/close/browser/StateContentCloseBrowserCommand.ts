/**
 * State Content Close Command - Browser Implementation (LOCAL-FIRST)
 *
 * PERFORMANCE OPTIMIZATION: Runs locally first, syncs to server in background.
 * This makes tab closing instant (<10ms) instead of waiting for server roundtrip.
 *
 * Flow:
 * 1. Load user state from localStorage (instant)
 * 2. Run removeContentItem locally
 * 3. Save to localStorage immediately
 * 4. Emit local event immediately (UI updates)
 * 5. Fire-and-forget sync to server
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { StateContentCloseParams, StateContentCloseResult } from '../shared/StateContentCloseTypes';
import { LocalStorageDataBackend } from '../../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import { UserStateEntity } from '../../../../../system/data/entities/UserStateEntity';
import { Events } from '../../../../../system/core/shared/Events';

const COLLECTION = 'user_states';

export class StateContentCloseBrowserCommand extends CommandBase<StateContentCloseParams, StateContentCloseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/close', context, subpath, commander);
  }

  async execute(params: StateContentCloseParams): Promise<StateContentCloseResult> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    const startTime = performance.now();

    try {
      // 1. Try to load user state from localStorage first (instant)
      const localResult = await LocalStorageDataBackend.list<UserStateEntity>(
        COLLECTION,
        { userId: params.userId }
      );

      let userState: UserStateEntity | null = null;

      if (localResult.success && localResult.entities && localResult.entities.length > 0) {
        userState = Object.assign(new UserStateEntity(), localResult.entities[0]);
        verbose && console.log(`⚡ state/content/close: Loaded from localStorage (${(performance.now() - startTime).toFixed(1)}ms)`);
      }

      // 2. If not in localStorage, fall back to server
      if (!userState) {
        verbose && console.log(`🔄 state/content/close: Not in localStorage, fetching from server...`);
        return await this.remoteExecute(params) as StateContentCloseResult;
      }

      // 3. Remove content item using UserStateEntity method
      userState.removeContentItem(params.contentItemId);

      // 4. Save to localStorage immediately
      await LocalStorageDataBackend.update(COLLECTION, userState.id, userState);

      // 5. Emit local event immediately (UI updates instantly)
      await Events.emit(this.context, 'content:closed', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: userState.contentState.currentItemId
      });

      const localTime = performance.now() - startTime;
      verbose && console.log(`⚡ state/content/close: LOCAL complete (${localTime.toFixed(1)}ms)`);

      // 6. Fire-and-forget sync to server (don't wait for it)
      this.remoteExecute(params).catch(error => {
        console.warn('state/content/close: Server sync failed (will retry on reconnect):', error);
      });

      // 7. Return success immediately (don't wait for server)
      return transformPayload(params, {
        success: true,
        openItemsCount: userState.contentState.openItems.length,
        currentItemId: userState.contentState.currentItemId
      });

    } catch (error) {
      console.error('❌ state/content/close: Local execution failed, falling back to server:', error);
      return await this.remoteExecute(params) as StateContentCloseResult;
    }
  }
}
