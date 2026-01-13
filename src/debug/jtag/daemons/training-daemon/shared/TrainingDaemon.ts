/**
 * TrainingDaemon - Automatic training data accumulation from conversations
 *
 * Silent observer that watches chat conversations in designated rooms (like #dev-updates)
 * and converts high-quality exchanges into training examples for continuous learning.
 *
 * ARCHITECTURE:
 * - Observes chat:message:sent events in training-enabled rooms
 * - Scores message quality (corrections > consensus > discussion)
 * - Accumulates TrainingExampleEntity records via universal ORM
 * - Triggers auto fine-tuning when threshold reached (e.g., 50+ examples)
 *
 * NATURAL INTEGRATION:
 * - No special APIs - AIs just talk naturally in rooms
 * - Hooks (GitHub, CI) post to rooms like Slack
 * - TrainingDaemon observes silently
 * - High-quality conversations → training data automatically
 *
 * VISION: Plastic collaborative intelligence - AIs get measurably better through real teamwork.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

export abstract class TrainingDaemon extends DaemonBase {
  public readonly subpath: string = 'training';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('training-daemon', context, router);
  }

  /**
   * Initialize daemon - override in server subclass for event subscriptions
   */
  abstract initialize(): Promise<void>;

  /**
   * Process messages - TrainingDaemon is event-driven, not message-driven
   * All actions happen via event subscriptions (chat:message:sent, etc.)
   */
  protected async processMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    // TrainingDaemon doesn't handle messages - it's purely event-driven
    return {
      context: message.payload.context,
      sessionId: message.payload.sessionId,
      success: false,
      timestamp: new Date().toISOString()
    };
  }
}
