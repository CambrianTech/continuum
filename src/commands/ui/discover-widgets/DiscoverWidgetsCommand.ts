/**
 * Discover Widgets Command - Server-side widget discovery
 * Uses WidgetDiscovery system to find all compliant widgets
 * Returns browser-accessible paths for dynamic loading
 */

import { BaseCommand } from '../../core/base-command/BaseCommand.js';
import { CommandResult } from '../../core/base-command/BaseCommand.js';
import { WidgetDiscovery } from '../../../ui/components/shared/WidgetDiscovery.js';

export class DiscoverWidgetsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'discover_widgets',
      description: 'Discover all compliant widgets for dynamic loading',
      category: 'ui',
      parameters: {},
      examples: [
        {
          description: 'Discover all widgets',
          command: 'discover_widgets',
          parameters: {}
        }
      ]
    };
  }

  static async execute(_params: any = {}): Promise<CommandResult> {
    try {
      const discovery = new WidgetDiscovery();
      const { compliant, nonCompliant } = await discovery.validateAllWidgets();
      
      // Generate browser-accessible paths
      const widgetPaths = discovery.generateWidgetPaths(compliant);
      
      // Log warnings for non-compliant widgets
      if (nonCompliant.length > 0) {
        console.warn(`⚠️ Found ${nonCompliant.length} non-compliant widget directories:`);
        for (const widget of nonCompliant) {
          console.warn(`  - ${widget.name}: ${widget.warnings.join(', ')}`);
        }
      }

      return {
        success: true,
        data: {
          widgets: widgetPaths,
          compliant: compliant.map(w => ({
            name: w.name,
            path: w.path,
            widgetFile: w.widgetFile,
            hasTests: w.testFiles.length > 0,
            hasCSS: w.cssFiles.length > 0
          })),
          nonCompliant: nonCompliant.map(w => ({
            name: w.name,
            warnings: w.warnings
          })),
          summary: {
            total: compliant.length + nonCompliant.length,
            compliant: compliant.length,
            nonCompliant: nonCompliant.length
          }
        },
        message: `Discovered ${compliant.length} compliant widgets, ${nonCompliant.length} non-compliant`
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Widget discovery failed: ${errorMessage}`,
        message: 'Widget discovery failed'
      };
    }
  }
}