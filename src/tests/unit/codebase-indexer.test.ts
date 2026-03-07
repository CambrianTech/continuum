/**
 * CodebaseIndexer Unit Tests
 *
 * Tests the chunking, file discovery, and embedding text generation
 * WITHOUT requiring Rust IPC or database. Pure structural tests.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import { CodebaseIndexer } from '../../system/rag/services/CodebaseIndexer';

// ── Test helpers ────────────────────────────────────────────────────────────

/**
 * Create a temp directory with test files for indexing
 */
function createTestFiles(tmpDir: string): void {
  fs.mkdirSync(tmpDir, { recursive: true });

  // TypeScript file with class and function
  fs.writeFileSync(path.join(tmpDir, 'Example.ts'), `
/**
 * Example class for testing
 */
export class ExampleService {
  private name: string;

  constructor(name: string) {
    this.name = name;
  }

  greet(): string {
    return \`Hello, \${this.name}!\`;
  }
}

export function helperFunction(x: number): number {
  return x * 2;
}

export interface ExampleConfig {
  timeout: number;
  retries: number;
}

export const DEFAULT_CONFIG: ExampleConfig = {
  timeout: 5000,
  retries: 3,
};
`);

  // Markdown file with sections
  fs.writeFileSync(path.join(tmpDir, 'README.md'), `# Test Project

## Overview

This is a test project for validating the CodebaseIndexer chunking logic.
It should be split into multiple chunks by heading level.

## Architecture

The system uses a modular architecture with:
- Commands for request/response
- Events for pub/sub
- Sentinels for long-running pipelines

### Key Components

Important components include the RAG system and Genome layer.

## API Reference

See the generated docs for API details.
`);

  // JavaScript file
  fs.writeFileSync(path.join(tmpDir, 'utils.js'), `
function formatDate(date) {
  return date.toISOString().split('T')[0];
}

const CONSTANTS = {
  MAX_RETRIES: 5,
  TIMEOUT_MS: 30000,
};

module.exports = { formatDate, CONSTANTS };
`);

  // File that should be skipped (node_modules)
  const nmDir = path.join(tmpDir, 'node_modules', 'fake-pkg');
  fs.mkdirSync(nmDir, { recursive: true });
  fs.writeFileSync(path.join(nmDir, 'index.ts'), 'export const skip = true;');

  // Non-indexable file
  fs.writeFileSync(path.join(tmpDir, 'data.json'), '{"skip": true}');
}

// ── Tests ───────────────────────────────────────────────────────────────────

describe('CodebaseIndexer', () => {
  let tmpDir: string;
  let indexer: CodebaseIndexer;

  beforeEach(() => {
    tmpDir = path.join('/tmp', `codebase-indexer-test-${Date.now()}`);
    createTestFiles(tmpDir);
    indexer = new CodebaseIndexer();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  describe('File Discovery', () => {
    it('should discover .ts, .md, .js files', () => {
      // Access private method via any cast for testing
      const files = (indexer as any).discoverFiles(tmpDir, true);

      const extensions = files.map((f: string) => path.extname(f));
      expect(extensions).toContain('.ts');
      expect(extensions).toContain('.md');
      expect(extensions).toContain('.js');
    });

    it('should skip node_modules', () => {
      const files: string[] = (indexer as any).discoverFiles(tmpDir, true);

      const nodeModulesFiles = files.filter((f: string) => f.includes('node_modules'));
      expect(nodeModulesFiles).toHaveLength(0);
    });

    it('should skip non-indexable extensions', () => {
      const files: string[] = (indexer as any).discoverFiles(tmpDir, true);

      const jsonFiles = files.filter((f: string) => f.endsWith('.json'));
      expect(jsonFiles).toHaveLength(0);
    });
  });

  describe('TypeScript Chunking', () => {
    it('should chunk by top-level declarations', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'Example.ts'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkTypeScript(content, 'Example.ts', 'typescript');

      // Should find: class ExampleService, function helperFunction, interface ExampleConfig, const DEFAULT_CONFIG
      expect(chunks.length).toBeGreaterThanOrEqual(3);

      const exportNames = chunks.map((c: any) => c.exportName);
      expect(exportNames).toContain('ExampleService');
      expect(exportNames).toContain('helperFunction');
      expect(exportNames).toContain('ExampleConfig');
    });

    it('should set correct exportType for each declaration', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'Example.ts'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkTypeScript(content, 'Example.ts', 'typescript');

      const classChunk = chunks.find((c: any) => c.exportName === 'ExampleService');
      expect(classChunk?.exportType).toBe('class');

      const funcChunk = chunks.find((c: any) => c.exportName === 'helperFunction');
      expect(funcChunk?.exportType).toBe('function');

      const ifaceChunk = chunks.find((c: any) => c.exportName === 'ExampleConfig');
      expect(ifaceChunk?.exportType).toBe('interface');
    });

    it('should include line numbers', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'Example.ts'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkTypeScript(content, 'Example.ts', 'typescript');

      for (const chunk of chunks) {
        expect(chunk.startLine).toBeGreaterThan(0);
        expect(chunk.endLine).toBeGreaterThanOrEqual(chunk.startLine);
      }
    });

    it('should set fileType correctly', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'Example.ts'), 'utf-8');
      const tsChunks: any[] = (indexer as any).chunkTypeScript(content, 'Example.ts', 'typescript');
      expect(tsChunks[0].fileType).toBe('typescript');

      const jsContent = fs.readFileSync(path.join(tmpDir, 'utils.js'), 'utf-8');
      const jsChunks: any[] = (indexer as any).chunkTypeScript(jsContent, 'utils.js', 'javascript');
      expect(jsChunks[0].fileType).toBe('javascript');
    });
  });

  describe('Markdown Chunking', () => {
    it('should chunk by headings', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'README.md'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkMarkdown(content, 'README.md');

      // Should split into sections: header, Overview, Architecture, Key Components, API Reference
      expect(chunks.length).toBeGreaterThanOrEqual(3);
    });

    it('should use heading text as exportName', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'README.md'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkMarkdown(content, 'README.md');

      const sectionNames = chunks.map((c: any) => c.exportName);
      expect(sectionNames).toContain('Overview');
      expect(sectionNames).toContain('Architecture');
    });

    it('should set exportType to markdown-section', () => {
      const content = fs.readFileSync(path.join(tmpDir, 'README.md'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkMarkdown(content, 'README.md');

      for (const chunk of chunks) {
        expect(chunk.exportType).toBe('markdown-section');
      }
    });

    it('should skip tiny sections (< 20 chars)', () => {
      const content = '# Title\n\nOK\n\n## Real Section\n\nThis section has enough content to be indexed properly.';
      const chunks: any[] = (indexer as any).chunkMarkdown(content, 'test.md');

      // "OK" section should be skipped (too short)
      const tinyChunks = chunks.filter((c: any) => c.content.trim() === 'OK');
      expect(tinyChunks).toHaveLength(0);
    });
  });

  describe('Embedding Text Generation', () => {
    it('should include file path and export name', () => {
      const chunk = {
        filePath: 'system/core/Commands.ts',
        fileType: 'typescript',
        content: 'export class Commands { execute() {} }',
        startLine: 1,
        endLine: 1,
        exportType: 'class',
        exportName: 'Commands',
      };

      const text: string = (indexer as any).chunkToEmbeddingText(chunk);

      expect(text).toContain('Commands');
      expect(text).toContain('system/core/Commands.ts');
      expect(text).toContain('execute');
    });

    it('should truncate long content for embedding input', () => {
      const longContent = 'x'.repeat(3000);
      const chunk = {
        filePath: 'long.ts',
        fileType: 'typescript',
        content: longContent,
        startLine: 1,
        endLine: 100,
      };

      const text: string = (indexer as any).chunkToEmbeddingText(chunk);

      // Should be truncated to ~1500 chars + file path
      expect(text.length).toBeLessThan(2000);
    });
  });

  describe('Chunk Content Limits', () => {
    it('should truncate chunks exceeding MAX_CHUNK_CHARS', () => {
      // Create a file with a very long class
      const longClass = `export class LongClass {\n${'  method() { return 1; }\n'.repeat(200)}}`;
      fs.writeFileSync(path.join(tmpDir, 'Long.ts'), longClass);

      const content = fs.readFileSync(path.join(tmpDir, 'Long.ts'), 'utf-8');
      const chunks: any[] = (indexer as any).chunkTypeScript(content, 'Long.ts', 'typescript');

      for (const chunk of chunks) {
        expect(chunk.content.length).toBeLessThanOrEqual(2000);
      }
    });
  });
});
