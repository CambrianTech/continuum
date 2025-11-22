/**
 * Screenshot Command - Server Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload, CommandResult } from '../../../system/core/types/JTAGTypes';
import { type ScreenshotParams, type ScreenshotResult, createScreenshotResult } from '../shared/ScreenshotTypes';
import { PersistenceError } from '../../../system/core/types/ErrorTypes';
import type { FileSaveParams, FileSaveResult } from '../../file/save/shared/FileSaveTypes';
import type { MediaItem } from '../../../system/data/entities/ChatMessageEntity';
import type { MediaResizeParams, MediaResizeResult } from '../../media/resize/shared/MediaResizeTypes';
import * as path from 'path';
import * as fs from 'fs';

export class ScreenshotServerCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  /**
   * Server does TWO things:
   * 1. If no image data → delegate to browser for capture
   * 2. If has image data → save to filesystem
   */
  async execute(params: JTAGPayload): Promise<ScreenshotResult> {
    const screenshotParams = params as ScreenshotParams;
    
    // console.debug(`📸 SERVER: Starting screenshot`);
    // console.debug(`🔍 DEBUG: ScreenshotServerCommand received sessionId: ${screenshotParams.sessionId}`);

    // Simple check: do we need browser capture?
    if (!screenshotParams.dataUrl) {
      // console.debug(`🔀 SERVER: Need image capture → delegating to browser`);
      return await this.remoteExecute(screenshotParams);
    }
    
    // We have image data  → delegate to file save command
    // we may have also called remoteExecute (above) to delegate to the browser and it called back to US via remoteExecute()
    // console.debug(`💾 SERVER: Saving image data (${screenshotParams.dataUrl.length} bytes)`);

    const filename = screenshotParams.filename ?? 'screenshot.png';
    const filepath = `screenshots/${filename}`;

    // Convert image data to buffer for file save command
    let content: Buffer | string;
    let finalDataUrl = screenshotParams.dataUrl; // May be resized

    if (screenshotParams.dataUrl) {
      // Check if we should resize for PersonaUsers with model config
      if (screenshotParams.context.callerType === 'persona' && screenshotParams.context.modelConfig?.model) {
        console.log(`📐 RESIZE: PersonaUser detected with model ${screenshotParams.context.modelConfig.model} - resizing screenshot`);

        // Save original to temp file first
        const tempFilename = `temp-screenshot-${Date.now()}.png`;
        const tempFilepath = `screenshots/${tempFilename}`;
        const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
        const tempContent = Buffer.from(base64Data, 'base64');

        const tempSaveParams: FileSaveParams = {
          ...screenshotParams,
          filepath: tempFilepath,
          content: tempContent
        };

        const tempSaveResult: FileSaveResult = await this.remoteExecute(tempSaveParams, 'file/save', this.context.environment);

        if (!tempSaveResult.success) {
          console.error(`❌ RESIZE: Failed to save temp file for resizing - using original`);
          // Fall back to original without resizing
          const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
          content = Buffer.from(base64Data, 'base64');
        } else {
          // Delegate to media/resize with model-aware sizing
          const resizeParams: MediaResizeParams = {
            ...screenshotParams,
            inputPath: tempSaveResult.filepath!,
            modelName: screenshotParams.context.modelConfig.model,
            targetPercentage: 0.12, // Use 12% of context window for images (with safety margin)
            returnBase64: true,
            quality: 80
          };

          const resizeResult: MediaResizeResult = await this.remoteExecute(resizeParams, 'media/resize', this.context.environment);

          if (resizeResult.success && resizeResult.base64) {
            console.log(`✅ RESIZE: ${resizeResult.originalDimensions!.width}x${resizeResult.originalDimensions!.height} → ${resizeResult.resizedDimensions!.width}x${resizeResult.resizedDimensions!.height} (${resizeResult.contextWindowPercentage!.toFixed(1)}% of context window)`);

            // Use resized data
            finalDataUrl = `data:image/png;base64,${resizeResult.base64}`;
            content = Buffer.from(resizeResult.base64, 'base64');

            // Clean up temp file
            try {
              const sessionPath = path.resolve(process.cwd(), '.continuum/sessions/user/shared/shared');
              fs.unlinkSync(path.join(sessionPath, tempSaveResult.filepath!));
            } catch (err) {
              console.warn(`⚠️  RESIZE: Could not delete temp file: ${err}`);
            }
          } else {
            console.error(`❌ RESIZE: Failed to resize image - using original`);
            const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
            content = Buffer.from(base64Data, 'base64');
          }
        }
      } else {
        // No resize needed - use original
        const base64Data = screenshotParams.dataUrl.replace(/^data:image\/\w+;base64,/, '');
        content = Buffer.from(base64Data, 'base64');
      }
      // console.debug(`📁 SERVER: Delegating binary image save to file command`);
    } else {
      content = `Screenshot captured at ${new Date().toISOString()}\nFilename: ${filename}\n`;
      // console.debug(`📝 SERVER: Delegating placeholder text save to file command`);
    }

    const saveParams: FileSaveParams = {
      ...screenshotParams,
      filepath: filepath,
      content: content
    };

    const saveResult: FileSaveResult = await this.remoteExecute(saveParams, 'file/save', this.context.environment);

    // console.debug(`🔍 SERVER: File save result:`, JSON.stringify(saveResult, null, 2));

    if (!saveResult.success) {
      throw new PersistenceError(filename, 'write', `File save command failed: ${JSON.stringify(saveResult)}`);
    }

    // Create base result (all callers get this)
    const baseResult = createScreenshotResult(screenshotParams.context, screenshotParams.sessionId, {
      success: true,
      filepath: saveResult.filepath,
      filename: filename,
      options: screenshotParams.options
    });

    // Prepare rich data for caller-adaptive output
    // PersonaUsers will receive this data in media field, HumanUsers won't
    // Use finalDataUrl which may be resized for PersonaUsers
    const richMediaData = finalDataUrl ? {
      base64Data: finalDataUrl.replace(/^data:image\/\w+;base64,/, ''),
      mimeType: finalDataUrl.match(/^data:(image\/\w+);base64,/)?.[1] ?? 'image/png',
      imageBytes: content instanceof Buffer ? content : undefined,
      metadata: {
        filename,
        filepath: saveResult.filepath,
        width: screenshotParams.metadata?.width,
        height: screenshotParams.metadata?.height,
        size: content instanceof Buffer ? content.length : undefined,
        querySelector: screenshotParams.querySelector,
        uploadedAt: Date.now(),
        uploadedBy: screenshotParams.sessionId
      }
    } : undefined;

    // Use caller-adaptive helper to populate media field for PersonaUsers only
    return await this.adaptResultForCaller(baseResult, screenshotParams.context, richMediaData);
  }

  /**
   * Override CommandBase.adaptResultForCaller to implement screenshot-specific media field population
   *
   * PersonaUsers receive image bytes in media field for vision systems
   * HumanUsers receive only filepath (no bytes)
   */
  protected async adaptResultForCaller<TResult extends CommandResult>(
    baseResult: TResult,
    context: JTAGContext,
    richData?: unknown
  ): Promise<TResult> {
    // Read caller type from context (populated by SessionDaemon during session creation)
    // If not set, default to 'script' (safest fallback - no assumptions)
    const callerType = context.callerType ?? 'script';

    // Type guard to validate richData structure
    const isRichMediaData = (data: unknown): data is {
      base64Data: string;
      mimeType: string;
      imageBytes?: Buffer;
      metadata: {
        filename: string;
        filepath: string;
        width?: number;
        height?: number;
        size?: number;
        querySelector?: string;
        uploadedAt: number;
        uploadedBy: string;
      };
    } => {
      return typeof data === 'object' && data !== null &&
        'base64Data' in data && 'mimeType' in data && 'metadata' in data;
    };

    // Only PersonaUsers get media field populated (vision capability)
    if (callerType === 'persona' && isRichMediaData(richData)) {
      const media: MediaItem = {
        type: 'image',
        base64: richData.base64Data,
        mimeType: richData.mimeType,
        filename: richData.metadata.filename,
        url: `file://${richData.metadata.filepath}`,
        width: richData.metadata.width,
        height: richData.metadata.height,
        size: richData.metadata.size,
        alt: `Screenshot captured at ${new Date().toISOString()}`,
        description: richData.metadata.querySelector
          ? `Screenshot of ${richData.metadata.querySelector}`
          : 'Screenshot',
        uploadedAt: richData.metadata.uploadedAt,
        uploadedBy: richData.metadata.uploadedBy
      };

      return { ...baseResult, media };
    }

    // HumanUsers and Scripts: just return base result (filepath only, no media bytes)
    return baseResult;
  }
}