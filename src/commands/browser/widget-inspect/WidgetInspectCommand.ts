/**
 * Widget Inspect Command - Comprehensive widget analysis and health reporting
 * 
 * Provides detailed inspection of widgets including:
 * - Content analysis (innerHTML, shadowDOM)
 * - Visual state (dimensions, visibility, styling)
 * - DOM health (connectivity, attributes, children)
 * - Performance metrics (load time, render state)
 * 
 * Used by JTAG for autonomous development validation.
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandResult } from '../../../types/CommandTypes';

export interface WidgetInspectOptions {
  selector?: string;
  includeContent?: boolean;
  includeStyling?: boolean;
  includeMetrics?: boolean;
  contentPreviewLength?: number;
  generateUUID?: boolean;
  timeout?: number;
}

export interface WidgetInfo {
  index: number;
  tagName: string;
  id: string | null;
  className: string | null;
  isConnected: boolean;
  hasContent: boolean;
  contentPreview: string;
  hasShadowRoot: boolean;
  shadowContent: string | null;
  attributes: Array<{ name: string; value: string }>;
  boundingBox: {
    width: number;
    height: number;
    x: number;
    y: number;
    visible: boolean;
  };
  styling?: {
    display: string;
    visibility: string;
    opacity: string;
    backgroundColor: string;
  };
  childElementCount: number;
  parentElement: string | null;
  error?: string;
}

export interface WidgetInspectionResult {
  inspectionUUID: string;
  timestamp: string;
  totalWidgets: number;
  pageTitle: string;
  pageUrl: string;
  selector: string;
  widgets: WidgetInfo[];
  performance?: {
    inspectionDuration: number;
    averageInspectionPerWidget: number;
  };
}

export class WidgetInspectCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'widget-inspect',
      description: 'Comprehensive widget inspection for debugging and autonomous development',
      category: 'browser',
      parameters: {
        selector: {
          type: 'string' as const,
          description: 'CSS selector for widgets to inspect',
          required: false,
          default: 'continuum-sidebar, chat-widget, [class*="widget"], [class*="Widget"]'
        },
        includeContent: {
          type: 'boolean' as const,
          description: 'Include widget content analysis',
          required: false,
          default: true
        },
        includeStyling: {
          type: 'boolean' as const,
          description: 'Include styling and visual state analysis',
          required: false,
          default: true
        },
        includeMetrics: {
          type: 'boolean' as const,
          description: 'Include performance metrics',
          required: false,
          default: true
        },
        contentPreviewLength: {
          type: 'number' as const,
          description: 'Length of content preview in characters',
          required: false,
          default: 200
        },
        generateUUID: {
          type: 'boolean' as const,
          description: 'Generate tracking UUID for inspection',
          required: false,
          default: true
        },
        timeout: {
          type: 'number' as const,
          description: 'Inspection timeout in milliseconds',
          required: false,
          default: 10000
        }
      },
      examples: [
        {
          description: 'Inspect all widgets with default settings',
          command: 'widget-inspect'
        },
        {
          description: 'Inspect specific widgets with custom selector',
          command: 'widget-inspect --selector=".sidebar-widget, .chat-widget"'
        },
        {
          description: 'Quick inspection without styling details',
          command: 'widget-inspect --includeStyling=false --contentPreviewLength=50'
        }
      ]
    };
  }

  static async execute(params: any, _context?: any): Promise<CommandResult> {
    try {
      const options = WidgetInspectCommand.parseParams(params);
      
      const {
        selector = 'continuum-sidebar, chat-widget, [class*="widget"], [class*="Widget"]',
        includeContent = true,
        includeStyling = true,
        includeMetrics = true,
        contentPreviewLength = 200,
        generateUUID = true,
        timeout = 10000
      } = options;

      // Generate inspection UUID
      const inspectionUUID = generateUUID ? 
        `inspect-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` : 
        'no-uuid';

      // Create JavaScript inspection script
      const inspectionScript = WidgetInspectCommand.createInspectionScript({
        selector,
        includeContent,
        includeStyling,
        includeMetrics,
        contentPreviewLength,
        inspectionUUID
      });

      // Execute via browser (similar to JSExecuteCommand pattern)
      const executionResult = await WidgetInspectCommand.executeInBrowser(inspectionScript, { timeout });

      return {
        success: true,
        data: {
          inspectionUUID,
          result: executionResult,
          timestamp: new Date().toISOString(),
          selector,
          options: { includeContent, includeStyling, includeMetrics, contentPreviewLength }
        },
        message: `Widget inspection completed ${inspectionUUID ? `[UUID: ${inspectionUUID}]` : ''}`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      return {
        success: false,
        error: `Widget inspection failed: ${errorMessage}`,
        data: {
          timestamp: new Date().toISOString()
        }
      };
    }
  }

  /**
   * Parse command parameters from various input formats
   */
  private static parseParams(params: any): WidgetInspectOptions {
    // Handle command line args (--key=value format)
    if (params && params.args && Array.isArray(params.args)) {
      const result: any = {};
      for (const arg of params.args) {
        if (typeof arg === 'string' && arg.startsWith('--')) {
          const [key, value] = arg.split('=', 2);
          const cleanKey = key.replace('--', '');
          
          // Convert string values to appropriate types
          if (value === 'true') result[cleanKey] = true;
          else if (value === 'false') result[cleanKey] = false;
          else if (!isNaN(Number(value))) result[cleanKey] = Number(value);
          else result[cleanKey] = value;
        }
      }
      return result;
    }
    
    // Handle object parameters
    if (typeof params === 'object' && params !== null) {
      return params;
    }
    
    // Handle JSON string
    if (typeof params === 'string') {
      try {
        return JSON.parse(params);
      } catch {
        return { selector: params }; // Treat as selector string
      }
    }
    
    return {};
  }

  /**
   * Create JavaScript inspection script with all the widget analysis logic
   */
  private static createInspectionScript(options: {
    selector: string;
    includeContent: boolean;
    includeStyling: boolean;
    includeMetrics: boolean;
    contentPreviewLength: number;
    inspectionUUID: string;
  }): string {
    const { selector, includeContent, includeStyling, includeMetrics, contentPreviewLength, inspectionUUID } = options;
    
    return `
      ${includeMetrics ? 'const inspectionStart = performance.now();' : ''}
      console.log("🔍 WIDGET_INSPECTION_START:${inspectionUUID}");
      
      try {
        const widgets = document.querySelectorAll('${selector}');
        const results = {
          inspectionUUID: "${inspectionUUID}",
          timestamp: new Date().toISOString(),
          totalWidgets: widgets.length,
          pageTitle: document.title,
          pageUrl: window.location.href,
          selector: "${selector}",
          widgets: Array.from(widgets).map((widget, index) => {
            ${includeMetrics ? 'const widgetStart = performance.now();' : ''}
            
            try {
              const rect = widget.getBoundingClientRect();
              ${includeStyling ? 'const computedStyle = window.getComputedStyle(widget);' : ''}
              
              const widgetInfo = {
                index: index,
                tagName: widget.tagName.toLowerCase(),
                id: widget.id || null,
                className: widget.className || null,
                isConnected: widget.isConnected,
                hasContent: widget.innerHTML.length > 0,
                hasShadowRoot: !!widget.shadowRoot,
                boundingBox: {
                  width: Math.round(rect.width),
                  height: Math.round(rect.height),
                  x: Math.round(rect.x),
                  y: Math.round(rect.y),
                  visible: rect.width > 0 && rect.height > 0
                },
                childElementCount: widget.childElementCount,
                parentElement: widget.parentElement ? widget.parentElement.tagName.toLowerCase() : null
              };
              
              ${includeContent ? `
              widgetInfo.contentPreview = widget.innerHTML.substring(0, ${contentPreviewLength}).replace(/\\s+/g, ' ').trim() || '[empty]';
              widgetInfo.shadowContent = widget.shadowRoot ? 
                widget.shadowRoot.innerHTML.substring(0, ${contentPreviewLength}).replace(/\\s+/g, ' ').trim() || '[empty shadow]' : 
                null;
              widgetInfo.attributes = Array.from(widget.attributes).map(attr => ({
                name: attr.name,
                value: attr.value
              }));
              ` : ''}
              
              ${includeStyling ? `
              widgetInfo.styling = {
                display: computedStyle.display,
                visibility: computedStyle.visibility,
                opacity: computedStyle.opacity,
                backgroundColor: computedStyle.backgroundColor
              };
              ` : ''}
              
              ${includeMetrics ? `
              widgetInfo.inspectionTime = performance.now() - widgetStart;
              ` : ''}
              
              return widgetInfo;
            } catch (error) {
              return {
                index: index,
                tagName: widget.tagName.toLowerCase(),
                error: 'Failed to inspect widget: ' + error.message
              };
            }
          })
        };
        
        ${includeMetrics ? `
        const inspectionEnd = performance.now();
        results.performance = {
          inspectionDuration: inspectionEnd - inspectionStart,
          averageInspectionPerWidget: widgets.length > 0 ? (inspectionEnd - inspectionStart) / widgets.length : 0
        };
        ` : ''}
        
        console.log("🔍 WIDGET_INSPECTION_COMPLETE:${inspectionUUID}");
        console.log("📊 WIDGET_SUMMARY: " + results.totalWidgets + " widgets found");
        
        JSON.stringify(results);
      } catch (error) {
        console.error("❌ WIDGET_INSPECTION_ERROR:${inspectionUUID}", error);
        JSON.stringify({
          inspectionUUID: "${inspectionUUID}",
          error: error.message,
          timestamp: new Date().toISOString()
        });
      }
    `;
  }

  /**
   * Execute script in browser - placeholder for real browser integration
   */
  private static async executeInBrowser(script: string, options: { timeout: number }): Promise<any> {
    // This would integrate with the browser execution system
    // For now, return a mock result indicating the script was prepared
    console.log(`🌍 Widget Inspection Script Prepared (${script.length} chars)`);
    
    // TODO: Integrate with existing browser execution infrastructure
    // This should use the same browser connection as JSExecuteCommand
    
    return {
      success: true,
      executedAt: new Date().toISOString(),
      method: 'browser-inspection-prepared',
      scriptLength: script.length,
      timeout: options.timeout,
      note: 'Widget inspection script prepared for browser execution'
    };
  }
}

export default WidgetInspectCommand;