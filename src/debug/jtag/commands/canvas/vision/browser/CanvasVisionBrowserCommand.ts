/**
 * Canvas Vision Browser Command
 *
 * Captures canvas content and delegates vision/transform to server
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type {
  CanvasVisionParams,
  CanvasVisionResult
} from '../shared/CanvasVisionTypes';
import { createCanvasVisionResult } from '../shared/CanvasVisionTypes';
import { DrawingCanvasWidget } from '@widgets/drawing-canvas/DrawingCanvasWidget';

export class CanvasVisionBrowserCommand extends CommandBase<CanvasVisionParams, CanvasVisionResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/vision', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasVisionResult> {
    const visionParams = params as CanvasVisionParams;
    let { imageBase64, action } = visionParams;

    console.log(`👁️ CanvasVision BROWSER: action=${action}, hasImage=${!!imageBase64}`);

    // If no image provided, capture from canvas widget
    if (!imageBase64) {
      console.log(`📸 CanvasVision BROWSER: Capturing canvas...`);
      const canvasWidget = this.findCanvasWidget();
      if (!canvasWidget) {
        console.error(`❌ CanvasVision BROWSER: No canvas widget found`);
        return createCanvasVisionResult(visionParams.context, visionParams.sessionId, action, {
          success: false,
          error: 'No canvas widget found and no imageBase64 provided'
        });
      }

      imageBase64 = canvasWidget.getCanvasBase64() || undefined;
      if (!imageBase64) {
        console.error(`❌ CanvasVision BROWSER: Failed to capture canvas`);
        return createCanvasVisionResult(visionParams.context, visionParams.sessionId, action, {
          success: false,
          error: 'Failed to capture canvas content'
        });
      }
      console.log(`✅ CanvasVision BROWSER: Captured ${imageBase64.length} bytes`);
    }

    // Delegate to server for API calls
    console.log(`🔀 CanvasVision BROWSER: Delegating to server...`);
    const serverParams: CanvasVisionParams = {
      ...visionParams,
      imageBase64
    };

    return await this.remoteExecute(serverParams);
  }

  /**
   * Find the drawing canvas widget in the DOM
   */
  private findCanvasWidget(): DrawingCanvasWidget | null {
    // Search through shadow DOMs
    const findDeep = (root: Element | Document): DrawingCanvasWidget | null => {
      const widget = root.querySelector('drawing-canvas-widget') as DrawingCanvasWidget;
      if (widget) return widget;

      for (const el of Array.from(root.querySelectorAll('*'))) {
        if (el.shadowRoot) {
          const found = findDeep(el.shadowRoot as unknown as Document);
          if (found) return found;
        }
      }
      return null;
    };

    return findDeep(document);
  }
}
