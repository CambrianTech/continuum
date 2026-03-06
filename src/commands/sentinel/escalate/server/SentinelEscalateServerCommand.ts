/**
 * Sentinel Escalate Command — Server Implementation
 *
 * Thin receiver for Rust sentinel lifecycle pushes. When a sentinel
 * completes/fails in Rust, it calls execute_ts_json("sentinel/escalate", ...)
 * and this command routes the notification to the owning persona's inbox.
 *
 * All tracking data originates in Rust — no TS-side polling or DB lookups.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelEscalateParams, SentinelEscalateResult } from '../shared/SentinelEscalateTypes';
import { handleSentinelEscalation } from '../../../../system/sentinel/SentinelEscalationService';

export class SentinelEscalateServerCommand extends CommandBase<SentinelEscalateParams, SentinelEscalateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/escalate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelEscalateResult> {
    const p = params as SentinelEscalateParams;

    if (!p.handle || !p.status) {
      return transformPayload(params, {
        success: false,
        processed: false,
        error: 'Missing handle or status',
      });
    }

    try {
      await handleSentinelEscalation(p);
      return transformPayload(params, { success: true, processed: true });
    } catch (err: any) {
      return transformPayload(params, {
        success: false,
        processed: false,
        error: err.message ?? String(err),
      });
    }
  }
}
