/**
 * FileSaveCommand Unit Tests
 * 
 * Tests for the FileSaveCommand functionality including base64 handling
 */

import { FileSaveCommand } from '../../FileSaveCommand';
import { CommandResult } from '../../../../core/base-command/BaseCommand';

describe('FileSaveCommand', () => {
  
  describe('getDefinition', () => {
    it('should return correct command definition', () => {
      const definition = FileSaveCommand.getDefinition();
      
      expect(definition.name).toBe('file_save');
      expect(definition.category).toBe('file');
      expect(definition.icon).toBe('💾');
      expect(definition.description).toContain('Save binary data');
      expect(definition.parameters.content).toBeDefined();
      expect(definition.parameters.filename).toBeDefined();
      expect(definition.parameters.encoding).toBeDefined();
      expect(definition.parameters.artifactType).toBeDefined();
    });
  });

  describe('execute', () => {
    it('should handle base64 encoded content', async () => {
      // Simple base64 encoded "Hello World" text
      const base64Content = Buffer.from('Hello World').toString('base64');
      
      const params = {
        content: base64Content,
        filename: 'test.txt',
        encoding: 'base64' as const,
        artifactType: 'file' as const
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('File saved successfully');
      expect(result.data?.filename).toBe('test.txt');
      expect(result.data?.encoding).toBe('base64');
    });

    it('should handle buffer content', async () => {
      const bufferContent = Buffer.from('Test buffer content');
      
      const params = {
        content: bufferContent,
        filename: 'buffer-test.bin',
        artifactType: 'file' as const
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('File saved successfully');
      expect(result.data?.filename).toBe('buffer-test.bin');
      expect(result.data?.size).toBe(bufferContent.length);
    });

    it('should handle string content with utf8 encoding', async () => {
      const stringContent = 'This is a test string';
      
      const params = {
        content: stringContent,
        filename: 'string-test.txt',
        encoding: 'utf8' as const,
        artifactType: 'file' as const
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('File saved successfully');
      expect(result.data?.filename).toBe('string-test.txt');
    });

    it('should default to screenshot artifact type', async () => {
      const params = {
        content: 'test content',
        filename: 'test.png'
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.data?.artifactType).toBe('screenshot');
    });

    it('should detect content type from file extension', async () => {
      const params = {
        content: 'test content',
        filename: 'test.png'
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.data?.contentType).toBe('image/png');
    });

    it('should handle session-specific saves', async () => {
      const params = {
        content: 'test content',
        filename: 'session-test.txt',
        sessionId: 'test-session-123',
        artifactType: 'file' as const
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('test-session-123');
    });

    it('should handle invalid content type', async () => {
      const params = {
        content: 123 as any, // Invalid content type
        filename: 'test.txt'
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Content must be a string or Buffer');
    });
  });

  describe('content type detection', () => {
    it('should detect various image formats', async () => {
      const testCases = [
        { filename: 'test.png', expectedType: 'image/png' },
        { filename: 'test.jpg', expectedType: 'image/jpeg' },
        { filename: 'test.jpeg', expectedType: 'image/jpeg' },
        { filename: 'test.gif', expectedType: 'image/gif' },
        { filename: 'test.svg', expectedType: 'image/svg+xml' },
        { filename: 'test.webp', expectedType: 'image/webp' }
      ];

      for (const testCase of testCases) {
        const params = {
          content: 'test content',
          filename: testCase.filename
        };

        const result = await FileSaveCommand.execute(params);
        
        expect(result.success).toBe(true);
        expect(result.data?.contentType).toBe(testCase.expectedType);
      }
    });

    it('should default to octet-stream for unknown extensions', async () => {
      const params = {
        content: 'test content',
        filename: 'test.unknown'
      };

      const result = await FileSaveCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.data?.contentType).toBe('application/octet-stream');
    });
  });
});