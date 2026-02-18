/**
 * Service Base - Foundation for Business Logic Services
 * 
 * Provides clean abstraction over the excellent existing router/transport system.
 * Services use this to send typed JTAGMessages without knowing transport details.
 * 
 * This is the bridge between clean API types and the existing daemon architecture.
 */

import type { JTAGMessage, JTAGContext } from '../../system/core/types/JTAGTypes';
import type { CommandParams, CommandResult } from '../../api/commands';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface IServiceTransport {
  sendCommand<TParams extends CommandParams, TResult extends CommandResult>(
    command: string,
    params: TParams,
    context?: JTAGContext
  ): Promise<TResult>;
  
  subscribeToEvents(eventType: string, handler: (data: any) => void): void;
  unsubscribeFromEvents(eventType: string, handler: (data: any) => void): void;
}

/**
 * Base class for all business logic services.
 * Uses existing router/transport system through clean abstraction.
 */
export abstract class ServiceBase {
  constructor(
    protected readonly serviceName: string,
    protected readonly transport: IServiceTransport,
    protected readonly context: JTAGContext
  ) {}

  /**
   * Execute command through existing router/transport system
   * Uses proper JTAGMessage format that router expects
   */
  protected async executeCommand<TParams extends CommandParams, TResult extends CommandResult>(
    command: string,
    params: TParams
  ): Promise<TResult> {
    try {
      const result = await this.transport.sendCommand<TParams, TResult>(
        command,
        params,
        this.context
      );
      
      if (!result.success) {
        throw new Error(`Command ${command} failed: ${result.error}`);
      }
      
      return result;
      
    } catch (error) {
      console.error(`❌ ${this.serviceName}: Command ${command} failed:`, error);
      throw error;
    }
  }

  /**
   * Subscribe to events through transport system
   */
  protected subscribeToEvents(eventType: string, handler: (data: any) => void): void {
    this.transport.subscribeToEvents(eventType, handler);
  }

  /**
   * Unsubscribe from events
   */
  protected unsubscribeFromEvents(eventType: string, handler: (data: any) => void): void {
    this.transport.unsubscribeFromEvents(eventType, handler);
  }
}

/**
 * Service Registry Interface - for dependency injection
 */
export interface IServiceRegistry {
  get<T>(serviceName: string): T | null;
  register<T>(serviceName: string, service: T): void;
}

/**
 * Simple service registry implementation
 */
export class ServiceRegistry implements IServiceRegistry {
  private services = new Map<string, any>();

  get<T>(serviceName: string): T | null {
    return this.services.get(serviceName) || null;
  }

  register<T>(serviceName: string, service: T): void {
    this.services.set(serviceName, service);
  }
}