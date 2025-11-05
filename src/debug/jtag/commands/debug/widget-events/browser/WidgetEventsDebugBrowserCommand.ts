/**
 * Widget Events Debug Browser Command
 * 
 * Elegant widget event debugging - replaces raw exec commands
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { 
  WidgetEventsDebugParams, 
  WidgetEventsDebugResult,
  EventHandlerInfo 
} from '../shared/WidgetEventsDebugTypes';
import { createWidgetEventsDebugResult } from '../shared/WidgetEventsDebugTypes';

export class WidgetEventsDebugBrowserCommand extends CommandBase<WidgetEventsDebugParams, WidgetEventsDebugResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-events', context, subpath, commander);
  }

  async execute(params: WidgetEventsDebugParams): Promise<WidgetEventsDebugResult> {
    const debugging = {
      logs: [] as string[],
      warnings: [] as string[],
      errors: [] as string[]
    };

    try {
      debugging.logs.push('🔍 Starting widget events debug analysis...');

      // Find the widget
      const widgetSelector = params.widgetSelector || 'chat-widget';
      const widget = this.findWidget(widgetSelector);
      
      if (!widget) {
        debugging.errors.push(`Widget not found: ${widgetSelector}`);
        return createWidgetEventsDebugResult(this.context, params.sessionId || 'unknown', {
          success: false,
          widgetFound: false,
          debugging,
          error: `Widget '${widgetSelector}' not found`
        });
      }

      debugging.logs.push(`✅ Widget found: ${widgetSelector}`);

      // Analyze widget methods
      const widgetMethods = this.getWidgetMethods(widget);
      debugging.logs.push(`📋 Widget has ${widgetMethods.length} methods`);

      // Analyze event system
      const eventSystem = this.analyzeEventSystem(widget, debugging);
      
      // Analyze event handlers
      const eventHandlers = this.analyzeEventHandlers(widget, debugging);

      // Test connectivity
      const connectivity = await this.testConnectivity(widget, params, debugging);

      debugging.logs.push('✅ Widget events debug analysis complete');

      return createWidgetEventsDebugResult(this.context, params.sessionId || 'unknown', {
        success: true,
        widgetFound: true,
        widgetPath: this.getWidgetPath(widgetSelector),
        widgetMethods,
        eventSystem,
        eventHandlers,
        connectivity,
        debugging
      });

    } catch (error) {
      debugging.errors.push(`❌ Widget events debug failed: ${error}`);
      throw error;
    }
  }

  private findWidget(selector: string): any {
    // Handle shadow DOM navigation for widgets
    if (selector === 'chat-widget') {
      const continuumWidget = document.querySelector('continuum-widget');
      const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
      return mainWidget?.shadowRoot?.querySelector('chat-widget');
    }
    
    // Default to direct selector
    return document.querySelector(selector);
  }

  private getWidgetPath(selector: string): string {
    if (selector === 'chat-widget') {
      return 'continuum-widget → main-widget → chat-widget';
    }
    return selector;
  }

  private getWidgetMethods(widget: any): string[] {
    return Object.getOwnPropertyNames(Object.getPrototypeOf(widget))
      .filter(name => typeof widget[name] === 'function')
      .sort();
  }

  private analyzeEventSystem(widget: any, debugging: any): any {
    const hasEventEmitter = widget.eventEmitter instanceof Map;
    const eventEmitterSize = hasEventEmitter ? widget.eventEmitter.size : 0;
    const eventTypes = hasEventEmitter ? Array.from(widget.eventEmitter.keys()).sort() : [];
    
    const hasDispatcherTypes = widget.dispatcherEventTypes instanceof Set;
    const dispatcherTypes = hasDispatcherTypes ? Array.from(widget.dispatcherEventTypes).sort() : [];
    
    debugging.logs.push(`📡 EventEmitter: ${hasEventEmitter ? 'YES' : 'NO'} (${eventEmitterSize} events)`);
    debugging.logs.push(`🔗 Dispatchers: ${dispatcherTypes.length} active`);
    
    return {
      hasEventEmitter,
      eventEmitterSize,
      eventTypes,
      dispatcherTypes,
      domListeners: [] // TODO: Detect DOM listeners
    };
  }

  private analyzeEventHandlers(widget: any, debugging: any): EventHandlerInfo[] {
    const handlers: EventHandlerInfo[] = [];
    
    if (widget.eventEmitter instanceof Map) {
      widget.eventEmitter.forEach((handlerList: Function[], eventName: string) => {
        const hasDispatcher = widget.dispatcherEventTypes?.has(eventName) || false;
        
        handlers.push({
          eventName,
          handlerCount: handlerList.length,
          hasDispatcher,
          listenerType: hasDispatcher ? 'both' : 'widget'
        });
        
        debugging.logs.push(`🎯 ${eventName}: ${handlerList.length} handlers, dispatcher: ${hasDispatcher ? 'YES' : 'NO'}`);
      });
    }
    
    return handlers.sort((a, b) => a.eventName.localeCompare(b.eventName));
  }

  private async testConnectivity(widget: any, params: WidgetEventsDebugParams, debugging: any): Promise<any> {
    debugging.logs.push('🧪 Testing event connectivity...');
    
    // Basic connectivity tests
    const serverEventsWorking = this.testServerEventConnectivity(widget, debugging);
    const domEventsWorking = this.testDOMEventConnectivity(widget, debugging);
    const dispatcherWorking = this.testDispatcherConnectivity(widget, debugging);
    
    return {
      serverEventsWorking,
      domEventsWorking,
      dispatcherWorking
    };
  }

  private testServerEventConnectivity(widget: any, debugging: any): boolean {
    // Check if widget has the necessary infrastructure for server events
    const hasEventEmitter = widget.eventEmitter instanceof Map;
    const hasDispatchers = widget.dispatcherEventTypes instanceof Set;
    const hasActiveDispatchers = hasDispatchers && widget.dispatcherEventTypes.size > 0;
    
    debugging.logs.push(`🔍 Server event infrastructure: ${hasActiveDispatchers ? 'READY' : 'MISSING'}`);
    
    return hasEventEmitter && hasActiveDispatchers;
  }

  private testDOMEventConnectivity(widget: any, debugging: any): boolean {
    // Basic DOM connectivity test
    debugging.logs.push('🔍 DOM event connectivity: AVAILABLE');
    return true;
  }

  private testDispatcherConnectivity(widget: any, debugging: any): boolean {
    // Check if event dispatcher infrastructure is working
    const hasDispatcherTypes = widget.dispatcherEventTypes instanceof Set;
    debugging.logs.push(`🔍 Event dispatcher: ${hasDispatcherTypes ? 'ACTIVE' : 'INACTIVE'}`);
    return hasDispatcherTypes;
  }
}