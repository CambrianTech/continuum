/**
 * Console Forwarding Types
 * Types specific to console message forwarding
 */

export interface ConsoleCommand extends Record<string, unknown> {
  action: string;
  message: string;
  timestamp: string;
  sessionId: string | null;
}