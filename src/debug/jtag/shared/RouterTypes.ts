/**
 * Router Response Types - Strong typing for JTAGRouter operations
 */

import type { TransportSendResult } from '../system/transports';

// Request handling result
export interface RequestResult {
  success: boolean;
  resolved?: boolean;
  response?: unknown;
}

// Event handling result  
export interface EventResult {
  success: boolean;
  delivered?: boolean;
  queued?: boolean;
  deduplicated?: boolean;
  willRetry?: boolean;
  priority?: string;
}

// Local routing result
export interface LocalRoutingResult {
  success: boolean;
  resolved?: boolean;
  error?: string;
}

// Union type for all router operations
export type RouterResult = TransportSendResult | RequestResult | EventResult | LocalRoutingResult;

// Type guards
export function isTransportSendResult(result: RouterResult): result is TransportSendResult {
  return 'timestamp' in result;
}

export function isRequestResult(result: RouterResult): result is RequestResult {
  return 'resolved' in result;
}

export function isEventResult(result: RouterResult): result is EventResult {
  return 'delivered' in result || 'queued' in result || 'deduplicated' in result;
}

export function isLocalRoutingResult(result: RouterResult): result is LocalRoutingResult {
  return 'error' in result || (!('timestamp' in result) && !('resolved' in result) && !('delivered' in result));
}