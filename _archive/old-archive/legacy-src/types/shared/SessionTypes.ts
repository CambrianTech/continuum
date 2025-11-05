// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Shared Session Types - Modern session management interfaces
 * 
 * ✅ CLEANED UP: Created proper shared types instead of any casts (2025-07-13)
 * ✅ CLEANED UP: Added modern web standards interfaces (2025-07-13)
 */

export interface SessionExtractionRequest {
  headers: Record<string, string>;
  url?: string;
  method?: string;
}

export interface SessionExtractionResponse {
  sessionId?: string;
  extractedFrom: 'header' | 'bearer' | 'cookie' | 'none';
  sessionExists: boolean;
}

export interface SessionInfo {
  sessionId?: string;
  userId?: string;
  source: 'header' | 'bearer' | 'cookie' | 'none';
}

export interface SessionValidationResult {
  valid: boolean;
  sessionId?: string;
  exists: boolean;
  error?: string;
}

export interface SessionContext {
  sessionId?: string;
  userId?: string;
  source?: 'http' | 'websocket' | 'cli';
  extractedFrom?: 'header' | 'bearer' | 'cookie' | 'none';
}