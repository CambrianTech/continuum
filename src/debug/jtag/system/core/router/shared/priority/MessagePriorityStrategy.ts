/**
 * MessagePriorityStrategy - Pluggable priority determination
 * 
 * PURPOSE: Allow configurable message priority rules
 * PATTERN: Strategy pattern for pluggable priority logic
 * EXTENSIBILITY: Easy to add new priority rules without changing router
 */

import type { JTAGMessage } from '../../../types/JTAGTypes';
import { MessagePriority } from '../queuing/JTAGMessageQueue';
import { MessageTypeGuards, type ConsoleMessagePayload } from '../MessageTypeGuards';
import { EVENT_PRIORITIES } from '../../../../events/shared/EventSystemConstants';
import { JTAG_ENDPOINTS } from '../JTAGEndpoints';

/**
 * Strategy interface for message priority determination
 */
export interface IMessagePriorityStrategy {
  determinePriority(message: JTAGMessage): MessagePriority;
}

/**
 * Default priority strategy - current hardcoded rules
 */
export class DefaultPriorityStrategy implements IMessagePriorityStrategy {
  
  determinePriority(message: JTAGMessage): MessagePriority {
    // System/health messages get critical priority
    if (message.origin.includes('system') || message.origin.includes('health')) {
      return MessagePriority.CRITICAL;
    }

    // Commands get high priority
    if (message.endpoint.includes('commands')) {
      return MessagePriority.HIGH;
    }

    // ALL events get critical priority for immediate cross-environment delivery (bypass health checks)
    if (message.endpoint.includes(JTAG_ENDPOINTS.EVENTS.BASE) ||
        message.endpoint.includes(JTAG_ENDPOINTS.EVENTS.BRIDGE) ||
        message.endpoint.includes(JTAG_ENDPOINTS.EVENTS.SYSTEM) ||
        message.endpoint.includes(JTAG_ENDPOINTS.EVENTS.CUSTOM)) {
      return MessagePriority.CRITICAL; // ALL events bypass queue and health checks
    }

    // All console messages get high priority for immediate delivery
    if (message.origin.includes('console') || message.endpoint.includes('console')) {
      return MessagePriority.HIGH;
    }

    return MessagePriority.NORMAL;
  }

  private isConsoleError(payload: unknown): boolean {
    return MessageTypeGuards.isConsoleError(payload);
  }
}

/**
 * Configurable priority strategy - rules can be adjusted
 */
export class ConfigurablePriorityStrategy implements IMessagePriorityStrategy {
  
  constructor(
    private rules: PriorityRule[] = []
  ) {}

  determinePriority(message: JTAGMessage): MessagePriority {
    // Apply rules in order - first match wins
    for (const rule of this.rules) {
      if (rule.matches(message)) {
        return rule.priority;
      }
    }
    
    return MessagePriority.NORMAL;
  }

  addRule(rule: PriorityRule): void {
    this.rules.push(rule);
  }

  clearRules(): void {
    this.rules = [];
  }
}

/**
 * Priority rule for configurable strategy
 */
export interface PriorityRule {
  matches(message: JTAGMessage): boolean;
  priority: MessagePriority;
  description?: string;
}

/**
 * Factory for common priority rules
 */
export class PriorityRuleFactory {
  
  static originContains(pattern: string, priority: MessagePriority): PriorityRule {
    return {
      matches: (message) => message.origin.includes(pattern),
      priority,
      description: `Origin contains '${pattern}'`
    };
  }

  static endpointContains(pattern: string, priority: MessagePriority): PriorityRule {
    return {
      matches: (message) => message.endpoint.includes(pattern),
      priority,
      description: `Endpoint contains '${pattern}'`
    };
  }

  static consoleLevel(level: ConsoleMessagePayload['level'], priority: MessagePriority): PriorityRule {
    return {
      matches: (message) => {
        const payload = message.payload;
        return MessageTypeGuards.isConsoleLevel(payload, level);
      },
      priority,
      description: `Console level is '${level}'`
    };
  }
}