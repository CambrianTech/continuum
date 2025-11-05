/**
 * Widget Base Types - Foundation for JTAG Web Component Widgets
 * 
 * Follows the same pattern as DaemonBase and CommandBase for consistent auto-discovery.
 */

import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';

// Extend Window interface to include widgetDaemon
declare global {
  interface Window {
    widgetDaemon?: {
      executeCommand(command: string, params: Omit<CommandParams, 'context' | 'sessionId'>): Promise<CommandResult>;
      isConnected(): boolean;
    };
  }
}

/**
 * Widget Entry - Auto-discovered by structure generator
 */
export interface WidgetEntry {
  name: string;
  className: string;
  widgetClass: new() => HTMLElement;
  tagName: string;
}

/**
 * Base Widget Class - Minimal foundation for web component widgets
 * 
 * NOTE: This is NOT the full widget implementation - just the interface
 * that allows widgets to be auto-discovered by the generator.
 * 
 * Real widgets should extend HTMLElement and use window.widgetDaemon
 * for JTAG command execution.
 */
export abstract class WidgetBase extends HTMLElement {
  /**
   * Widget name for auto-discovery
   */
  static get widgetName(): string {
    throw new Error('Subclasses must implement static widgetName getter');
  }

  /**
   * HTML tag name for registration
   */
  static get tagName(): string {
    return this.widgetName + '-widget';
  }

  constructor() {
    super();
  }

  /**
   * Execute command via WidgetDaemon
   */
  protected async executeCommand(command: string, params: Omit<CommandParams, 'context' | 'sessionId'> = {}): Promise<CommandResult> {
    const widgetDaemon = window.widgetDaemon;
    if (!widgetDaemon) {
      throw new Error('WidgetDaemon not available - ensure JTAG system is running');
    }
    
    return await widgetDaemon.executeCommand(command, params);
  }
}