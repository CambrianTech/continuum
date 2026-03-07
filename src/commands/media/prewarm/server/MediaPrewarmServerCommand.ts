/**
 * Media Prewarm Command - Server Implementation
 *
 * Pre-warm vision description cache for image media. Fires
 * VisionDescriptionService.describeBase64() so that by the time personas
 * build RAG context, descriptions are cached or in-flight.
 *
 * Called fire-and-forget by chat/send when images are attached.
 * LLaVA takes 60-70s on CPU — starting inference at upload time
 * means descriptions are cached before the NEXT message triggers RAG.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MediaPrewarmParams, MediaPrewarmResult } from '../shared/MediaPrewarmTypes';
import { createMediaPrewarmResultFromParams } from '../shared/MediaPrewarmTypes';
import { VisionDescriptionService } from '@system/vision/VisionDescriptionService';

export class MediaPrewarmServerCommand extends CommandBase<MediaPrewarmParams, MediaPrewarmResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('media/prewarm', context, subpath, commander);
  }

  async execute(params: MediaPrewarmParams): Promise<MediaPrewarmResult> {
    const images = params.images;
    if (!images || images.length === 0) {
      return createMediaPrewarmResultFromParams(params, { success: true, queued: 0 });
    }

    const service = VisionDescriptionService.getInstance();
    if (!service.isAvailable()) {
      return createMediaPrewarmResultFromParams(params, { success: true, queued: 0 });
    }

    let queued = 0;
    for (const img of images) {
      if (!img.base64) continue;
      // Fire and forget — the promise resolves into the VisionDescriptionService cache
      service.describeBase64(img.base64, img.mimeType ?? 'image/png', {
        maxLength: 500,
        detectText: true,
      }).catch(() => {
        // Best-effort — swallow errors
      });
      queued++;
    }

    return createMediaPrewarmResultFromParams(params, { success: true, queued });
  }
}
