/**
 * JTAG Event System - Promise-Powered Event Architecture
 */

import type { JTAGMessage, JTAGContext } from '../../core/types/JTAGTypes';
import { type UUID } from '../../core/types/CrossPlatformUUID';
import type { SystemEventData, SystemEventName } from './SystemEvents';

export interface EventsInterface<T = unknown> {
  emit(eventName: string, data?: T): void;
  on(eventName: string, listener: (data?: T) => void): () => void;
  waitFor?(eventName: string, timeout?: number): Promise<T>;
}

export class EventManager {
  listeners: Map<string, ((data?: unknown) => void)[]> = new Map();

  get events(): EventsInterface {
    return {
      emit: (eventName: string, data?: unknown) => {
        const listeners = this.listeners.get(eventName) || [];
        listeners.forEach(listener => listener(data));
      },
      
      on: (eventName: string, listener: (data?: unknown) => void): (() => void) => {
        if (!this.listeners.has(eventName)) {
          this.listeners.set(eventName, []);
        }
        this.listeners.get(eventName)!.push(listener);
        
        // Return unsubscribe function
        return () => {
          const listeners = this.listeners.get(eventName);
          if (listeners) {
            const index = listeners.indexOf(listener);
            if (index > -1) {
              listeners.splice(index, 1);
            }
          }
        };
      },
      
      waitFor: async (eventName: string, timeout = 5000): Promise<unknown> => {
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => {
            reject(new Error(`Timeout waiting for event: ${eventName}`));
          }, timeout);
          
          const unsubscribe = this.events.on(eventName, (data) => {
            clearTimeout(timer);
            unsubscribe();
            resolve(data);
          });
        });
      }
    };
  }
}