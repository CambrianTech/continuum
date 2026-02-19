/**
 * File Processing Utilities
 * 
 * Modular utilities for file discovery, processing, and batch operations.
 */

import * as fs from 'fs';
import * as path from 'path';

export interface FileProcessingResult {
  filePath: string;
  modified: boolean;
  replacements: Array<{ from: string; to: string }>;
}

export interface ProcessingStats {
  filesProcessed: number;
  filesModified: number;
  totalReplacements: number;
}

/**
 * Discovers JavaScript files recursively
 */
export class FileDiscovery {
  static findJavaScriptFiles(rootDir: string): string[] {
    const files: string[] = [];
    
    function traverse(currentDir: string): void {
      const items = fs.readdirSync(currentDir);
      
      for (const item of items) {
        const itemPath = path.join(currentDir, item);
        const stat = fs.statSync(itemPath);
        
        if (stat.isDirectory()) {
          traverse(itemPath);
        } else if (item.endsWith('.js')) {
          files.push(path.relative(rootDir, itemPath));
        }
      }
    }
    
    traverse(rootDir);
    return files;
  }
}

/**
 * Processes individual files with transformation functions
 */
export class FileProcessor {
  static processFile<T>(
    filePath: string,
    transformer: (content: string, filePath: string) => { content: string; metadata: T }
  ): { modified: boolean; metadata: T } {
    const content = fs.readFileSync(filePath, 'utf8');
    const { content: newContent, metadata } = transformer(content, filePath);
    
    const modified = content !== newContent;
    if (modified) {
      fs.writeFileSync(filePath, newContent, 'utf8');
    }
    
    return { modified, metadata };
  }
}

/**
 * Batch processor for multiple files
 */
export class BatchProcessor {
  private stats: ProcessingStats = {
    filesProcessed: 0,
    filesModified: 0,
    totalReplacements: 0
  };

  /**
   * Process multiple files with a transformation function
   */
  processFiles<T>(
    files: string[],
    rootDir: string,
    transformer: (content: string, filePath: string) => { content: string; metadata: T },
    onFileProcessed?: (result: { filePath: string; modified: boolean; metadata: T }) => void
  ): ProcessingStats {
    for (const file of files) {
      const fullPath = path.join(rootDir, file);
      
      try {
        const { modified, metadata } = FileProcessor.processFile(fullPath, transformer);
        
        this.stats.filesProcessed++;
        if (modified) {
          this.stats.filesModified++;
        }
        
        // Call callback with results
        onFileProcessed?.({ filePath: file, modified, metadata });
        
      } catch (error) {
        console.error(`❌ Error processing ${file}: ${error}`);
      }
    }
    
    return this.stats;
  }

  /**
   * Get current processing statistics
   */
  getStats(): ProcessingStats {
    return { ...this.stats };
  }

  /**
   * Reset statistics
   */
  resetStats(): void {
    this.stats = {
      filesProcessed: 0,
      filesModified: 0,
      totalReplacements: 0
    };
  }
}