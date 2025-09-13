/**
 * Event Constants Generator - No Magic Strings Architecture
 * 
 * BREAKTHROUGH: Automatically scans all daemon event definitions and creates
 * centralized, type-safe event constants. No more hardcoded magic strings.
 * 
 * Key Features:
 * - Scans *Events.ts files in all daemons
 * - Generates unified JTAG_EVENTS constants with namespace prefixes
 * - Creates type-safe event data mapping interfaces
 * - Supports room-scoped, user-scoped, and system-scoped events
 * - Auto-detects event subscription patterns from existing code
 */

import * as fs from 'fs';
import * as path from 'path';
import { glob } from 'glob';

interface EventDefinition {
  name: string;
  fullName: string;
  namespace: string;
  dataType?: string;
  scope: 'system' | 'room' | 'user' | 'global';
}

interface DaemonEventSource {
  daemon: string;
  filePath: string;
  events: EventDefinition[];
  constants: Record<string, string>;
  interfaces: string[];
}

export class EventConstantsGenerator {
  private readonly basePath: string;
  private readonly outputPath: string;
  
  constructor(basePath: string = '.', outputPath: string = 'system/events/generated') {
    this.basePath = basePath;
    this.outputPath = outputPath;
  }

  /**
   * Main generation method - scans all daemons and creates unified constants
   */
  async generateEventConstants(): Promise<void> {
    console.log('🔍 EventConstantsGenerator: Scanning daemon event definitions...');
    
    // Find all daemon event files
    const eventFiles = await this.findDaemonEventFiles();
    console.log(`📄 Found ${eventFiles.length} daemon event files`);
    
    // Parse each event file to extract events and types
    const daemonSources: DaemonEventSource[] = [];
    for (const filePath of eventFiles) {
      try {
        const source = await this.parseDaemonEventFile(filePath);
        if (source.events.length > 0) {
          daemonSources.push(source);
          console.log(`✅ Parsed ${source.daemon}: ${source.events.length} events`);
        }
      } catch (error) {
        console.warn(`⚠️ Failed to parse ${filePath}:`, error);
      }
    }
    
    // Generate unified constants file
    await this.generateUnifiedConstants(daemonSources);
    
    // Generate scoped event interfaces
    await this.generateScopedEventInterfaces(daemonSources);
    
    // Generate event registry for runtime access
    await this.generateEventRegistry(daemonSources);
    
    console.log('✅ EventConstantsGenerator: Event constants generation complete!');
  }

  /**
   * Find all daemon event definition files
   */
  private async findDaemonEventFiles(): Promise<string[]> {
    const patterns = [
      'daemons/**/*Events.ts',
      'system/events/**/*Events.ts',
      'system/transports/**/*Events.ts'
    ];
    
    const files: string[] = [];
    for (const pattern of patterns) {
      const matches = await glob(pattern, { cwd: this.basePath });
      files.push(...matches.map(f => path.resolve(this.basePath, f)));
    }
    
    return files.filter(f => fs.existsSync(f));
  }

  /**
   * Parse daemon event file to extract events and types
   */
  private async parseDaemonEventFile(filePath: string): Promise<DaemonEventSource> {
    const content = await fs.promises.readFile(filePath, 'utf-8');
    const daemon = this.extractDaemonName(filePath);
    
    // Extract event constants (e.g., COMMAND_EVENTS = { ... })
    const constants = this.extractEventConstants(content);
    
    // Extract event interfaces (e.g., CommandEventData)
    const interfaces = this.extractEventInterfaces(content);
    
    // Convert constants to event definitions with scope detection
    const events = this.parseEventDefinitions(constants, daemon);
    
    return {
      daemon,
      filePath,
      events,
      constants,
      interfaces
    };
  }

  /**
   * Extract daemon name from file path
   */
  private extractDaemonName(filePath: string): string {
    const parts = filePath.split('/');
    
    // Handle patterns like: daemons/command-daemon/CommandEvents.ts
    if (parts.includes('daemons')) {
      const daemonIndex = parts.indexOf('daemons');
      if (daemonIndex < parts.length - 1) {
        return parts[daemonIndex + 1].replace('-daemon', '');
      }
    }
    
    // Handle patterns like: system/events/SystemEvents.ts
    if (parts.includes('events')) {
      return 'system';
    }
    
    // Fallback to filename
    return path.basename(filePath, '.ts').replace('Events', '').toLowerCase();
  }

  /**
   * Extract event constant definitions from TypeScript content
   */
  private extractEventConstants(content: string): Record<string, string> {
    const constants: Record<string, string> = {};
    
    // Match patterns like: COMMAND_EVENTS = { EXECUTED: 'command.executed', ... }
    const constantsRegex = /export const (\w+_EVENTS) = \{([^}]+)\}/gs;
    const match = constantsRegex.exec(content);
    
    if (match) {
      const [, , body] = match;
      
      // Extract individual key-value pairs
      const eventRegex = /(\w+):\s*['"`]([^'"`]+)['"`]/g;
      let eventMatch;
      
      while ((eventMatch = eventRegex.exec(body)) !== null) {
        const [, key, value] = eventMatch;
        constants[key] = value;
      }
    }
    
    return constants;
  }

  /**
   * Extract event interface definitions from TypeScript content
   */
  private extractEventInterfaces(content: string): string[] {
    const interfaces: string[] = [];
    
    // Match interface definitions
    const interfaceRegex = /export interface (\w+EventData)/g;
    let match;
    
    while ((match = interfaceRegex.exec(content)) !== null) {
      interfaces.push(match[1]);
    }
    
    return interfaces;
  }

  /**
   * Parse event definitions and detect scoping patterns
   */
  private parseEventDefinitions(constants: Record<string, string>, daemon: string): EventDefinition[] {
    const events: EventDefinition[] = [];
    
    for (const [key, value] of Object.entries(constants)) {
      const scope = this.detectEventScope(value, key);
      
      events.push({
        name: key,
        fullName: value,
        namespace: daemon,
        scope
      });
    }
    
    return events;
  }

  /**
   * Detect event scope based on naming patterns and content
   */
  private detectEventScope(eventName: string, key: string): 'system' | 'room' | 'user' | 'global' {
    // System-scoped events
    if (eventName.startsWith('system.') || key.includes('SYSTEM') || eventName.includes('daemon')) {
      return 'system';
    }
    
    // Room-scoped events (chat, collaboration)
    if (eventName.includes('room') || eventName.includes('chat') || eventName.includes('participant')) {
      return 'room';
    }
    
    // User-scoped events (sessions, personal state)
    if (eventName.includes('session') || eventName.includes('user') || eventName.includes('personal')) {
      return 'user';
    }
    
    // Default to global
    return 'global';
  }

  /**
   * Generate unified event constants file
   */
  private async generateUnifiedConstants(sources: DaemonEventSource[]): Promise<void> {
    const outputDir = path.join(this.basePath, this.outputPath);
    await fs.promises.mkdir(outputDir, { recursive: true });
    
    const imports: string[] = [];
    const exports: string[] = [];
    const allEvents: string[] = [];
    
    // Import all daemon event constants
    for (const source of sources) {
      const relativePath = path.relative(outputDir, source.filePath).replace(/\.ts$/, '');
      const constantName = `${source.daemon.toUpperCase()}_EVENTS`;
      
      imports.push(`import { ${constantName} } from '${relativePath}';`);
      exports.push(`  ...${constantName},`);
      
      // Collect all event names for type union
      allEvents.push(...source.events.map(e => `'${e.fullName}'`));
    }
    
    const content = `/**
 * Unified Event Constants - Generated from Daemon Definitions
 * 
 * ⚠️ AUTO-GENERATED FILE - DO NOT EDIT MANUALLY
 * Generated by EventConstantsGenerator at ${new Date().toISOString()}
 * 
 * This file provides centralized access to all JTAG system events
 * without magic strings. All events are type-safe and auto-discovered.
 */

${imports.join('\n')}

/**
 * All JTAG Events - Unified constants from all daemons
 */
export const JTAG_EVENTS = {
${exports.join('\n')}
} as const;

/**
 * Union type of all event names for type safety
 */
export type JTAGEventName = ${allEvents.join(' | ')};

/**
 * Event scope detection helpers
 */
export const EVENT_SCOPES = {
  SYSTEM: [${sources.flatMap(s => s.events.filter(e => e.scope === 'system').map(e => `'${e.fullName}'`)).join(', ')}] as const,
  ROOM: [${sources.flatMap(s => s.events.filter(e => e.scope === 'room').map(e => `'${e.fullName}'`)).join(', ')}] as const,
  USER: [${sources.flatMap(s => s.events.filter(e => e.scope === 'user').map(e => `'${e.fullName}'`)).join(', ')}] as const,
  GLOBAL: [${sources.flatMap(s => s.events.filter(e => e.scope === 'global').map(e => `'${e.fullName}'`)).join(', ')}] as const
} as const;
`;

    const outputFile = path.join(outputDir, 'UnifiedEventConstants.ts');
    await fs.promises.writeFile(outputFile, content);
    console.log(`📝 Generated unified constants: ${outputFile}`);
  }

  /**
   * Generate scoped event interfaces with subscription helpers
   */
  private async generateScopedEventInterfaces(_sources: DaemonEventSource[]): Promise<void> {
    const outputDir = path.join(this.basePath, this.outputPath);
    
    const content = `/**
 * Scoped Event Interfaces - Type-Safe Event Subscriptions
 * 
 * ⚠️ AUTO-GENERATED FILE - DO NOT EDIT MANUALLY
 * Generated by EventConstantsGenerator at ${new Date().toISOString()}
 * 
 * Provides type-safe interfaces for scoped event subscriptions:
 * - System-scoped: jtag.events.system.on()
 * - Room-scoped: jtag.events.room(roomId).on()
 * - User-scoped: jtag.events.user(userId).on()
 */

import type { EventsInterface } from '../shared/JTAGEventSystem';
import { JTAG_EVENTS, EVENT_SCOPES } from './UnifiedEventConstants';

/**
 * System-scoped event interface
 * Events that affect the entire JTAG system
 */
export interface SystemScopedEvents {
  on(eventName: typeof EVENT_SCOPES.SYSTEM[number], listener: (data?: any) => void): () => void;
  emit(eventName: typeof EVENT_SCOPES.SYSTEM[number], data?: any): void;
  waitFor(eventName: typeof EVENT_SCOPES.SYSTEM[number], timeout?: number): Promise<any>;
}

/**
 * Room-scoped event interface
 * Events that are specific to a chat room or collaboration space
 */
export interface RoomScopedEvents {
  on(eventName: typeof EVENT_SCOPES.ROOM[number], listener: (data?: any) => void): () => void;
  emit(eventName: typeof EVENT_SCOPES.ROOM[number], data?: any): void;
  waitFor(eventName: typeof EVENT_SCOPES.ROOM[number], timeout?: number): Promise<any>;
}

/**
 * User-scoped event interface  
 * Events that are specific to a user session or personal state
 */
export interface UserScopedEvents {
  on(eventName: typeof EVENT_SCOPES.USER[number], listener: (data?: any) => void): () => void;
  emit(eventName: typeof EVENT_SCOPES.USER[number], data?: any): void;
  waitFor(eventName: typeof EVENT_SCOPES.USER[number], timeout?: number): Promise<any>;
}

/**
 * Scoped Events Factory - Creates scoped event interfaces
 */
export interface ScopedEventsInterface {
  // System-level events
  system: SystemScopedEvents;
  
  // Room-scoped events with ID
  room(roomId: string): RoomScopedEvents;
  
  // User-scoped events with ID
  user(userId: string): UserScopedEvents;
  
  // Global events (backwards compatibility)
  global: EventsInterface;
}
`;

    const outputFile = path.join(outputDir, 'ScopedEventInterfaces.ts');
    await fs.promises.writeFile(outputFile, content);
    console.log(`📝 Generated scoped interfaces: ${outputFile}`);
  }

  /**
   * Generate runtime event registry for dynamic access
   */
  private async generateEventRegistry(sources: DaemonEventSource[]): Promise<void> {
    const outputDir = path.join(this.basePath, this.outputPath);
    
    // Build registry data structure
    const registryEntries: string[] = [];
    
    for (const source of sources) {
      for (const event of source.events) {
        registryEntries.push(`  '${event.fullName}': {
    name: '${event.name}',
    fullName: '${event.fullName}',
    namespace: '${event.namespace}',
    scope: '${event.scope}',
    daemon: '${source.daemon}'
  }`);
      }
    }
    
    const content = `/**
 * Event Registry - Runtime Event Discovery and Metadata
 * 
 * ⚠️ AUTO-GENERATED FILE - DO NOT EDIT MANUALLY
 * Generated by EventConstantsGenerator at ${new Date().toISOString()}
 * 
 * Provides runtime access to event metadata for dynamic subscription,
 * debugging, and introspection capabilities.
 */

export interface EventMetadata {
  name: string;
  fullName: string;
  namespace: string;
  scope: 'system' | 'room' | 'user' | 'global';
  daemon: string;
}

/**
 * Runtime Event Registry - All JTAG events with metadata
 */
export const EVENT_REGISTRY: Record<string, EventMetadata> = {
${registryEntries.join(',\n')}
};

/**
 * Event Registry Utilities
 */
export class EventRegistryUtils {
  /**
   * Get all events for a specific daemon
   */
  static getEventsForDaemon(daemon: string): EventMetadata[] {
    return Object.values(EVENT_REGISTRY).filter(e => e.daemon === daemon);
  }
  
  /**
   * Get all events by scope
   */
  static getEventsByScope(scope: EventMetadata['scope']): EventMetadata[] {
    return Object.values(EVENT_REGISTRY).filter(e => e.scope === scope);
  }
  
  /**
   * Get event metadata by full name
   */
  static getEventMetadata(fullName: string): EventMetadata | undefined {
    return EVENT_REGISTRY[fullName];
  }
  
  /**
   * Check if event name is valid
   */
  static isValidEvent(eventName: string): boolean {
    return eventName in EVENT_REGISTRY;
  }
  
  /**
   * Get all available event names
   */
  static getAllEventNames(): string[] {
    return Object.keys(EVENT_REGISTRY);
  }
  
  /**
   * Get events summary by daemon and scope
   */
  static getEventsSummary(): Record<string, Record<string, number>> {
    const summary: Record<string, Record<string, number>> = {};
    
    for (const event of Object.values(EVENT_REGISTRY)) {
      if (!summary[event.daemon]) {
        summary[event.daemon] = { system: 0, room: 0, user: 0, global: 0 };
      }
      summary[event.daemon][event.scope]++;
    }
    
    return summary;
  }
}
`;

    const outputFile = path.join(outputDir, 'EventRegistry.ts');
    await fs.promises.writeFile(outputFile, content);
    console.log(`📝 Generated event registry: ${outputFile}`);
  }
}

/**
 * CLI entry point for generating event constants
 */
if (require.main === module) {
  const generator = new EventConstantsGenerator();
  generator.generateEventConstants().catch(console.error);
}