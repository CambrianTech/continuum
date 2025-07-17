/**
 * Log Entry Formatter Tests - Test universal log formatting
 * Tests consistent formatting across all contexts
 */

import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert';
import { UniversalLogEntryFormatter } from '../shared/LogEntryFormatter';
import { BaseLogEntry } from '../shared/LoggingTypes';
import { continuumContextFactory } from '../../types/shared/core/ContinuumTypes';

describe('UniversalLogEntryFormatter', () => {
  let formatter: UniversalLogEntryFormatter;
  let mockLogEntry: BaseLogEntry;

  beforeEach(() => {
    formatter = new UniversalLogEntryFormatter();
    mockLogEntry = {
      level: 'info',
      message: 'Test log message',
      timestamp: '2025-01-01T00:00:00.000Z',
      source: 'test-source',
      context: continuumContextFactory.create({
        sessionId: 'test-session-123' as any,
        environment: 'server'
      })
    };
  });

  describe('Human Readable Formatting', () => {
    it('should format human readable log entry', () => {
      const formatted = formatter.formatHuman(mockLogEntry);
      
      assert.strictEqual(formatted, 
        'UL: [2025-01-01T00:00:00.000Z] [test-source] INFO: Test log message [session:test-session-123]'
      );
    });

    it('should handle different log levels', () => {
      const levels = ['debug', 'info', 'warn', 'error'] as const;
      
      levels.forEach(level => {
        const entry = { ...mockLogEntry, level };
        const formatted = formatter.formatHuman(entry);
        
        assert.ok(formatted.includes(level.toUpperCase()));
      });
    });

    it('should handle missing session ID', () => {
      const entryWithoutSession = {
        ...mockLogEntry,
        context: continuumContextFactory.create({
          environment: 'server'
        })
      };
      
      const formatted = formatter.formatHuman(entryWithoutSession);
      
      expect(formatted).toContain(`[session:${entryWithoutSession.context.sessionId}]`);
    });
  });

  describe('JSON Formatting', () => {
    it('should format JSON log entry', () => {
      const formatted = formatter.formatJSON(mockLogEntry);
      const parsed = JSON.parse(formatted);
      
      expect(parsed).toEqual({
        level: 'info',
        message: 'Test log message',
        timestamp: '2025-01-01T00:00:00.000Z',
        source: 'test-source',
        context: mockLogEntry.context,
        metadata: undefined
      });
    });

    it('should include metadata when present', () => {
      const entryWithMetadata = {
        ...mockLogEntry,
        metadata: { userId: 'test-123', action: 'login' }
      };
      
      const formatted = formatter.formatJSON(entryWithMetadata);
      const parsed = JSON.parse(formatted);
      
      expect(parsed.metadata).toEqual({
        userId: 'test-123',
        action: 'login'
      });
    });
  });

  describe('Console Formatting', () => {
    it('should format console log entry', () => {
      const formatted = formatter.formatConsole(mockLogEntry);
      
      expect(formatted).toBe(
        '[INFO] [test-source] Test log message [session:test-session-123]'
      );
    });

    it('should handle unknown source', () => {
      const entryWithUnknownSource = {
        ...mockLogEntry,
        source: 'unknown'
      };
      
      const formatted = formatter.formatConsole(entryWithUnknownSource);
      
      expect(formatted).toBe(
        '[INFO] Test log message [session:test-session-123]'
      );
    });

    it('should handle missing session info', () => {
      const entryWithoutSession = {
        ...mockLogEntry,
        context: continuumContextFactory.create({
          environment: 'server'
        })
      };
      
      const formatted = formatter.formatConsole(entryWithoutSession);
      
      expect(formatted).toContain('[INFO] [test-source] Test log message');
    });
  });

  describe('Stack Formatting', () => {
    it('should format with execution stack', () => {
      const contextWithStack = continuumContextFactory.create({
        sessionId: 'test-session-123',
        environment: 'server',
        executionStack: [
          {
            environment: 'server',
            location: 'UserService',
            description: 'login method',
            timestamp: '2025-01-01T00:00:00.000Z'
          },
          {
            environment: 'server',
            location: 'DatabaseService',
            description: 'authenticate',
            timestamp: '2025-01-01T00:00:00.001Z'
          }
        ]
      });
      
      const entryWithStack = {
        ...mockLogEntry,
        context: contextWithStack
      };
      
      const formatted = formatter.formatWithStack(entryWithStack);
      
      expect(formatted).toContain('[stack: server:UserService (login method) → server:DatabaseService (authenticate)]');
    });

    it('should handle stack without descriptions', () => {
      const contextWithStack = continuumContextFactory.create({
        sessionId: 'test-session-123',
        environment: 'server',
        executionStack: [
          {
            environment: 'server',
            location: 'UserService',
            timestamp: '2025-01-01T00:00:00.000Z'
          }
        ]
      });
      
      const entryWithStack = {
        ...mockLogEntry,
        context: contextWithStack
      };
      
      const formatted = formatter.formatWithStack(entryWithStack);
      
      expect(formatted).toContain('[stack: server:UserService]');
    });

    it('should format normally without stack', () => {
      const formatted = formatter.formatWithStack(mockLogEntry);
      
      expect(formatted).toBe(formatter.formatHuman(mockLogEntry));
    });
  });

  describe('Target-Specific Formatting', () => {
    it('should format for file target', () => {
      const formatted = formatter.formatForTarget(mockLogEntry, 'file');
      
      expect(formatted).toBe(formatter.formatHuman(mockLogEntry));
    });

    it('should format for console target', () => {
      const formatted = formatter.formatForTarget(mockLogEntry, 'console');
      
      expect(formatted).toBe(formatter.formatConsole(mockLogEntry));
    });

    it('should format for JSON target', () => {
      const formatted = formatter.formatForTarget(mockLogEntry, 'json');
      
      expect(formatted).toBe(formatter.formatJSON(mockLogEntry));
    });

    it('should format for stack target', () => {
      const formatted = formatter.formatForTarget(mockLogEntry, 'stack');
      
      expect(formatted).toBe(formatter.formatWithStack(mockLogEntry));
    });

    it('should default to human format for unknown target', () => {
      const formatted = formatter.formatForTarget(mockLogEntry, 'unknown' as any);
      
      expect(formatted).toBe(formatter.formatHuman(mockLogEntry));
    });
  });
});