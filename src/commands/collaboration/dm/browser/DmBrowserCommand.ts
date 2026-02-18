/**
 * DM Command - Browser Implementation
 * Delegates to server for actual room creation/lookup
 */

import { DmCommand } from '../shared/DmCommand';
import type { DmParams, DmResult } from '../shared/DmTypes';

export class DmBrowserCommand extends DmCommand {

  protected async executeDm(params: DmParams): Promise<DmResult> {
    // Browser delegates to server
    throw new Error('DM command must run on server');
  }
}
