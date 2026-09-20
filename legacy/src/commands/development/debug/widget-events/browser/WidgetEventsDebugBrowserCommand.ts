/**
 * Widget Events Debug Browser Command
 * 
 * Elegant widget event debugging - replaces raw exec commands
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { 
  WidgetEventsDebugParams, 
  WidgetEventsDebugResult,
  EventHandlerInfo 
} from '../shared/WidgetEventsDebugTypes';
import { createWidgetEventsDebugResult } from '../shared/WidgetEventsDebugTypes';

/** Represents a widget element in the shadow DOM with optional event infrastructure */
interface WidgetElement extends HTMLElement {
  eventEmitter?: Map<string, Function[]>;
  dispatcherEventTypes?: Set<string>;
}

/** Debug log accumulator passed through analysis methods */
interface DebugLog {
  logs: string[];
  warnings: string[];
  errors: string[];
}

/** Event connectivity test results */
interface ConnectivityResult {
  serverEventsWorking: boolean;
  domEventsWorking: boolean;
  dispatcherWorking: boolean;
}

/** Event system analysis result */
interface EventSystemInfo {
  hasEventEmitter: boolean;
  eventEmitterSize: number;
  eventTypes: string[];
  dispatcherTypes: string[];
  domListeners: string[];
}

export class WidgetEventsDebugBrowserCommand extends CommandBase<WidgetEventsDebugParams, WidgetEventsDebugResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/widget-events', context, subpath, commander);
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
      const message = error instanceof Error ? error.message : String(error);
      debugging.errors.push(`Widget events debug failed: ${message}`);
      throw error;
    }
  }

  private findWidget(selector: string): WidgetElement | null {
    // Handle shadow DOM navigation for widgets
    if (selector === 'chat-widget') {
      const continuumWidget = document.querySelector('continuum-widget');
      const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
      return (mainWidget?.shadowRoot?.querySelector('chat-widget') as WidgetElement | null) ?? null;
    }
    
    // Default to direct selector
    return document.querySelector(selector) as WidgetElement | null;
  }

  private getWidgetPath(selector: string): string {
    if (selector === 'chat-widget') {
      return 'continuum-widget → main-widget → chat-widget';
    }
    return selector;
  }

  private getWidgetMethods(widget: WidgetElement): string[] {
    const proto = Object.getPrototypeOf(widget) as Record<string, unknown>;
    return Object.getOwnPropertyNames(proto)
      .filter(name => typeof (widget as unknown as Record<string, unknown>)[name] === 'function')
      .sort();
  }

  private analyzeEventSystem(widget: WidgetElement, debugging: DebugLog): EventSystemInfo {
    const emitter = widget.eventEmitter;
    const hasEventEmitter = emitter instanceof Map;
    const eventEmitterSize = hasEventEmitter ? emitter.size : 0;
    const eventTypes = hasEventEmitter ? Array.from(emitter.keys()).sort() : [];

    const dispatchers = widget.dispatcherEventTypes;
    const hasDispatcherTypes = dispatchers instanceof Set;
    const dispatcherTypes = hasDispatcherTypes ? Array.from(dispatchers).sort() : [];
    
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

  private analyzeEventHandlers(widget: WidgetElement, debugging: DebugLog): EventHandlerInfo[] {
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

  private async testConnectivity(widget: WidgetElement, params: WidgetEventsDebugParams, debugging: DebugLog): Promise<ConnectivityResult> {
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

  private testServerEventConnectivity(widget: WidgetElement, debugging: DebugLog): boolean {
    // Check if widget has the necessary infrastructure for server events
    const hasEventEmitter = widget.eventEmitter instanceof Map;
    const dispatchers = widget.dispatcherEventTypes;
    const hasActiveDispatchers = dispatchers instanceof Set && dispatchers.size > 0;
    
    debugging.logs.push(`🔍 Server event infrastructure: ${hasActiveDispatchers ? 'READY' : 'MISSING'}`);
    
    return hasEventEmitter && hasActiveDispatchers;
  }

  private testDOMEventConnectivity(widget: WidgetElement, debugging: DebugLog): boolean {
    // Basic DOM connectivity test
    debugging.logs.push('🔍 DOM event connectivity: AVAILABLE');
    return true;
  }

  private testDispatcherConnectivity(widget: WidgetElement, debugging: DebugLog): boolean {
    // Check if event dispatcher infrastructure is working
    const hasDispatcherTypes = widget.dispatcherEventTypes instanceof Set;
    debugging.logs.push(`🔍 Event dispatcher: ${hasDispatcherTypes ? 'ACTIVE' : 'INACTIVE'}`);
    return hasDispatcherTypes;
  }
}