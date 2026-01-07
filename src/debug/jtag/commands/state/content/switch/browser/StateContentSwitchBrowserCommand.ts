/**
 * State Content Switch Command - Browser Implementation (LOCAL-FIRST)
 *
 * PERFORMANCE OPTIMIZATION: Runs locally first, syncs to server in background.
 * This makes tab switching instant (<10ms) instead of waiting for server roundtrip.
 *
 * Flow:
 * 1. Load user state from localStorage (instant)
 * 2. Run setCurrentContent locally
 * 3. Save to localStorage immediately
 * 4. Emit local event immediately (UI updates)
 * 5. Fire-and-forget sync to server
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../shared/StateContentSwitchTypes';
import { LocalStorageDataBackend } from '../../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import { UserStateEntity } from '../../../../../system/data/entities/UserStateEntity';
import { Events } from '../../../../../system/core/shared/Events';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

const COLLECTION = 'user_states';

export class StateContentSwitchBrowserCommand extends CommandBase<StateContentSwitchParams, StateContentSwitchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/switch', context, subpath, commander);
  }

  async execute(params: StateContentSwitchParams): Promise<StateContentSwitchResult> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    const startTime = performance.now();

    try {
      // 1. Try to load user state from localStorage first (instant)
      const localResult = await LocalStorageDataBackend.list<UserStateEntity>(
        COLLECTION,
        { userId: params.userId }
      );

      let userState: UserStateEntity | null = null;
      let fromLocal = false;

      if (localResult.success && localResult.entities && localResult.entities.length > 0) {
        userState = Object.assign(new UserStateEntity(), localResult.entities[0]);
        fromLocal = true;
        verbose && console.log(`⚡ state/content/switch: Loaded from localStorage (${(performance.now() - startTime).toFixed(1)}ms)`);
      }

      // 2. If not in localStorage, we need to fetch from server first
      if (!userState) {
        verbose && console.log(`🔄 state/content/switch: Not in localStorage, fetching from server...`);
        // Fall back to server - this will be slow but only happens once per session
        return await this.remoteExecute(params) as StateContentSwitchResult;
      }

      // 3. Run setCurrentContent locally
      const switched = userState.setCurrentContent(params.contentItemId);

      if (!switched) {
        return transformPayload(params, {
          success: false,
          currentItemId: userState.contentState.currentItemId || '' as UUID,
          error: `Content item not found in openItems: ${params.contentItemId}`
        });
      }

      // 4. Save to localStorage immediately
      await LocalStorageDataBackend.update(COLLECTION, userState.id, userState);

      // 5. Get content item details for the event
      const contentItem = userState.contentState.openItems.find(
        item => item.id === params.contentItemId
      );

      // 6. Emit local event immediately (UI updates instantly)
      await Events.emit(this.context, 'content:switched', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: userState.contentState.currentItemId,
        contentType: contentItem?.type,
        entityId: contentItem?.entityId,
        title: contentItem?.title
      });

      const localTime = performance.now() - startTime;
      verbose && console.log(`⚡ state/content/switch: LOCAL complete (${localTime.toFixed(1)}ms, from ${fromLocal ? 'localStorage' : 'memory'})`);

      // 7. Fire-and-forget sync to server (don't wait for it)
      this.remoteExecute(params).catch(error => {
        console.warn('state/content/switch: Server sync failed (will retry on reconnect):', error);
      });

      // 8. Return success immediately (don't wait for server)
      return transformPayload(params, {
        success: true,
        currentItemId: userState.contentState.currentItemId!,
        contentType: contentItem?.type,
        entityId: contentItem?.entityId,
        title: contentItem?.title
      });

    } catch (error) {
      console.error('❌ state/content/switch: Local execution failed, falling back to server:', error);
      // On any error, fall back to server
      return await this.remoteExecute(params) as StateContentSwitchResult;
    }
  }
}
