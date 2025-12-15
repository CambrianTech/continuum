/**
 * Widget CSS Debug Server Command
 * Orchestrates CSS injection and optional screenshot
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WidgetCSSDebugParams, WidgetCSSDebugResult } from '../shared/WidgetCSSDebugTypes';
import { createWidgetCSSDebugResult } from '../shared/WidgetCSSDebugTypes';
import type { ScreenshotResult } from '@commands/interface/screenshot/shared/ScreenshotTypes';

export class WidgetCSSServerCommand extends CommandBase<WidgetCSSDebugParams, WidgetCSSDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-css-debug', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<WidgetCSSDebugResult> {
    const cssParams = params as WidgetCSSDebugParams;

    try {
      // EXTRACT MODE: Just call browser and return
      if (cssParams.extract) {
        const browserResult = await this.remoteExecute(cssParams) as WidgetCSSDebugResult;

        // When extracting, output raw CSS for easy redirect
        if (browserResult.success && browserResult.extractedCSS) {
          console.log(browserResult.extractedCSS);
        }

        return browserResult;
      }

      // INJECTION MODE: If cssFile provided, read it
      let cssContent = cssParams.cssContent;
      if (cssParams.cssFile) {
        try {
          const fs = await import('fs/promises');
          const path = await import('path');

          // Resolve relative paths
          const cssPath = path.isAbsolute(cssParams.cssFile)
            ? cssParams.cssFile
            : path.resolve(process.cwd(), cssParams.cssFile);

          cssContent = await fs.readFile(cssPath, 'utf-8');
          console.log(`📄 Loaded CSS from ${cssPath}`);
        } catch (readError) {
          return createWidgetCSSDebugResult(this.context, this.context.uuid, {
            success: false,
            widgetSelector: cssParams.widgetSelector,
            cssInjected: false,
            error: `Failed to read CSS file: ${readError instanceof Error ? readError.message : String(readError)}`
          });
        }
      }

      // Pass cssContent to browser command
      const browserParams = {
        ...cssParams,
        cssContent
      };

      // Execute browser command to inject CSS
      const browserResult = await this.remoteExecute(browserParams) as WidgetCSSDebugResult;

      if (!browserResult.success) {
        return browserResult;
      }

      // Take screenshot if requested (default: true)
      const takeScreenshot = cssParams.screenshot !== false;
      if (takeScreenshot) {
        const filename = cssParams.filename || `widget-css-${cssParams.widgetSelector}.png`;

        const screenshotResult = await this.remoteExecute({
          context: this.context,
          sessionId: this.context.uuid,
          querySelector: cssParams.widgetSelector,
          filename
        }, 'screenshot') as ScreenshotResult;

        if (screenshotResult.success) {
          return {
            ...browserResult,
            screenshotTaken: true,
            screenshotPath: screenshotResult.filepath
          };
        }
      }

      return browserResult;

    } catch (error) {
      return createWidgetCSSDebugResult(this.context, this.context.uuid, {
        success: false,
        widgetSelector: cssParams.widgetSelector,
        cssInjected: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
