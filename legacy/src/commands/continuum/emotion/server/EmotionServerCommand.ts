import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { EmotionParams, EmotionResult } from '../shared/EmotionTypes';
import { EMOTION_EVENT } from '../shared/EmotionTypes';
import { Events } from '../../../../system/core/shared/Events';

export class EmotionServerCommand extends CommandBase<EmotionParams, EmotionResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('continuum/emotion', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<EmotionResult> {
    const emotionParams = params as EmotionParams;
    const emoji = emotionParams.emoji;
    const color = emotionParams.color || '#00d9ff'; // Default cyan
    const duration = emotionParams.duration || 3000; // Default 3 seconds

    // Emit event that emoter widget will listen for (with context for proper routing)
    await Events.emit(this.context, EMOTION_EVENT, {
      emoji,
      color,
      duration,
      timestamp: Date.now()
    });

    return transformPayload(emotionParams, {
      success: true,
      emoji,
      color,
      duration,
      timestamp: new Date().toISOString()
    });
  }
}
