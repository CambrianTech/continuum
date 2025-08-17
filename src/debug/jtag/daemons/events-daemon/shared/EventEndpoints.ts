/**
 * Events Daemon Endpoint Constants
 * 
 * Single source of truth for all event-related endpoint paths
 */

export const EVENT_ENDPOINTS = {
  BRIDGE: 'event-bridge',
  STATS: 'stats'
} as const;

export type EventEndpoint = typeof EVENT_ENDPOINTS[keyof typeof EVENT_ENDPOINTS];