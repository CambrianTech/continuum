/**
 * Widget List Command - Dynamic widget discovery for browser
 * Returns available widget paths based on what's actually compiled and available
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand';
import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
import { promises as fs } from 'fs';
import { join } from 'path';

export class WidgetListCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'widget-list',
      description: 'Get list of available compiled widgets for dynamic loading',
      category: 'ui',
      examples: [
        {
          description: 'Get all available widgets',
          command: 'widget-list'
        }
      ],
      parameters: {}
    };
  }

  async execute(_params: any, _context: ContinuumContext): Promise<CommandResult> {
    try {
      const widgets = await this.discoverAvailableWidgets();
      
      return {
        success: true,
        message: `Found ${widgets.length} available widgets`,
        data: { widgets }
      };
    } catch (error) {
      return {
        success: false,
        message: `Widget discovery failed: ${error instanceof Error ? error.message : String(error)}`,
        error: `Widget discovery failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  private async discoverAvailableWidgets(): Promise<string[]> {
    const widgets: string[] = [];
    
    // Use the actual working path from the server
    const distPath = 'dist/ui/components';
    
    try {
      const components = await this.scanForWidgets(distPath);
      widgets.push(...components);
      console.log(`✅ Found ${components.length} compiled widgets in ${distPath}`);
    } catch (error) {
      console.warn(`⚠️ No compiled widgets found in ${distPath}:`, error instanceof Error ? error.message : String(error));
      
      // Fallback to src if dist not available
      try {
        const srcComponents = await this.scanForWidgets('src/ui/components');
        widgets.push(...srcComponents);
        console.log(`📁 Using source widgets from src/ui/components`);
      } catch (srcError) {
        console.error(`❌ No widgets found in either location`);
      }
    }
    
    return widgets;
  }

  private async scanForWidgets(basePath: string): Promise<string[]> {
    const widgets: string[] = [];
    
    try {
      const componentDirs = await fs.readdir(basePath);
      
      for (const dir of componentDirs) {
        if (dir === 'shared' || dir === 'test') continue;
        
        const dirPath = join(basePath, dir);
        const stat = await fs.stat(dirPath);
        
        if (stat.isDirectory()) {
          // Look for Widget.js files in this directory
          try {
            const files = await fs.readdir(dirPath);
            const widgetFile = files.find(f => f.endsWith('Widget.js'));
            
            if (widgetFile) {
              // Use /src/ paths since UserSelector.js works from there
              const widgetPath = `/src/ui/components/${dir}/${widgetFile}`;
              
              widgets.push(widgetPath);
              console.log(`✅ Found widget: ${widgetPath}`);
            }
          } catch (err) {
            // Skip directories we can't read
            continue;
          }
        }
      }
    } catch (error) {
      throw new Error(`Failed to scan ${basePath}: ${error instanceof Error ? error.message : String(error)}`);
    }
    
    return widgets;
  }
}