/**
 * LogsShared - Shared utilities for all log commands
 */

import * as path from 'path';
import type { LogMetadata } from '../../../system/core/logging/LogFileRegistry';

/**
 * Convert log file path to simple identifier
 */
export function pathToLogName(filePath: string): string {
  const normalized = path.normalize(filePath);

  // External logs
  if (!normalized.includes('.continuum')) {
    return path.basename(filePath, '.log');
  }

  // System logs
  if (normalized.includes('/jtag/logs/system/')) {
    const filename = path.basename(filePath, '.log');
    return `system/${filename}`;
  }

  // Persona logs
  if (normalized.includes('/personas/')) {
    const parts = normalized.split('/personas/')[1].split('/');
    const personaId = parts[0];
    const filename = path.basename(filePath, '.log');
    return `${personaId}/${filename}`;
  }

  // Session logs
  if (normalized.includes('/sessions/')) {
    const parts = normalized.split('/sessions/')[1].split('/');
    const runId = parts[1];
    const filename = path.basename(filePath, '.log');
    return `session/${runId}/${filename}`;
  }

  return path.basename(filePath, '.log');
}

/**
 * Convert log name back to file path
 */
export function logNameToPath(logName: string, allLogs: LogMetadata[]): string | null {
  const log = allLogs.find(l => pathToLogName(l.filePath) === logName);
  return log ? log.filePath : null;
}

/**
 * Resolve persona ID
 */
export async function resolvePersonaId(personaId?: string, personaUniqueId?: string): Promise<string | null> {
  if (personaUniqueId) return personaUniqueId;
  if (personaId) return personaId;
  return null;
}

/**
 * Parse relative time strings
 */
export function parseRelativeTime(timeStr: string): Date {
  const now = new Date();

  const relativeMatch = timeStr.match(/^(\d+)(h|d|m|s)$/);
  if (relativeMatch) {
    const value = parseInt(relativeMatch[1]);
    const unit = relativeMatch[2];

    switch (unit) {
      case 's': return new Date(now.getTime() - value * 1000);
      case 'm': return new Date(now.getTime() - value * 60 * 1000);
      case 'h': return new Date(now.getTime() - value * 60 * 60 * 1000);
      case 'd': return new Date(now.getTime() - value * 24 * 60 * 60 * 1000);
    }
  }

  if (timeStr === 'today') {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    return today;
  }

  if (timeStr === 'yesterday') {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    yesterday.setHours(0, 0, 0, 0);
    return yesterday;
  }

  return new Date(timeStr);
}
