#!/usr/bin/env npx tsx
/**
 * Hot Reload File Watcher - Autonomous Development Support
 * ========================================================
 * Watches widget files and automatically rebuilds + reloads browser
 */

import { spawn } from 'child_process';
import { createReadStream } from 'fs';
import { createInterface } from 'readline';

export class HotReloadWatcher {
  private watching = false;
  private reloadTimeout: NodeJS.Timeout | null = null;

  async start() {
    console.log('👁️  Hot Reload Watcher: Starting...');
    console.log('====================================');
    console.log('🔍 Watching: src/ui/components/**/*.ts');
    console.log('🔄 Auto hot-reload on file changes');
    console.log('⏹️  Press Ctrl+C to stop');
    console.log('');
    
    this.watching = true;
    
    // Use simple polling approach to avoid chokidar dependency
    this.startPolling();
    
    // Handle graceful shutdown
    process.on('SIGINT', () => {
      console.log('\n⏹️  Stopping hot reload watcher...');
      this.watching = false;
      process.exit(0);
    });
  }

  private async startPolling() {
    const watchedFiles = new Map<string, number>();
    
    // Initial scan
    await this.scanFiles(watchedFiles);
    
    // Poll for changes every 2 seconds
    const pollInterval = setInterval(async () => {
      if (!this.watching) {
        clearInterval(pollInterval);
        return;
      }
      
      await this.checkForChanges(watchedFiles);
    }, 2000);
  }

  private async scanFiles(fileMap: Map<string, number>) {
    try {
      const find = spawn('find', ['src/ui/components', '-name', '*.ts', '-type', 'f']);
      const rl = createInterface({ input: find.stdout });
      
      for await (const line of rl) {
        const filePath = line.trim();
        if (filePath && !filePath.includes('.d.ts')) {
          try {
            const stat = await import('fs').then(fs => fs.promises.stat(filePath));
            fileMap.set(filePath, stat.mtimeMs);
          } catch (error) {
            // File might have been deleted
          }
        }
      }
    } catch (error) {
      console.log(`❌ Error scanning files: ${error}`);
    }
  }

  private async checkForChanges(fileMap: Map<string, number>) {
    let hasChanges = false;
    const changedFiles: string[] = [];
    
    for (const [filePath, lastMtime] of fileMap) {
      try {
        const stat = await import('fs').then(fs => fs.promises.stat(filePath));
        if (stat.mtimeMs > lastMtime) {
          fileMap.set(filePath, stat.mtimeMs);
          changedFiles.push(filePath);
          hasChanges = true;
        }
      } catch (error) {
        // File might have been deleted, remove from map
        fileMap.delete(filePath);
      }
    }
    
    // Also check for new files
    const currentFiles = new Map<string, number>();
    await this.scanFiles(currentFiles);
    
    for (const [filePath, mtime] of currentFiles) {
      if (!fileMap.has(filePath)) {
        fileMap.set(filePath, mtime);
        changedFiles.push(filePath);
        hasChanges = true;
      }
    }
    
    if (hasChanges) {
      console.log(`📝 Changes detected:`);
      changedFiles.forEach(file => console.log(`   ${file}`));
      
      // Debounce rapid changes
      if (this.reloadTimeout) {
        clearTimeout(this.reloadTimeout);
      }
      
      this.reloadTimeout = setTimeout(async () => {
        console.log('🔥 Triggering hot reload...');
        await this.performHotReload();
        console.log('👁️  Watching for more changes...');
        console.log('');
      }, 1000); // 1 second debounce
    }
  }

  private async performHotReload(): Promise<void> {
    try {
      // Rebuild browser bundle
      const build = spawn('npm', ['run', 'build:browser-hot'], { stdio: 'inherit' });
      
      build.on('close', async (code) => {
        if (code === 0) {
          console.log('✅ Build complete, reloading browser...');
          
          // Reload the browser
          try {
            const fetch = await import('node-fetch').then(m => m.default);
            const response = await fetch('http://localhost:9000/api/commands/reload', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ target: 'page' })
            });
            
            const result = await response.json() as any;
            if (result.success) {
              console.log('🔄 Browser reloaded successfully');
            } else {
              console.log('❌ Failed to reload browser:', result.error);
            }
          } catch (reloadError) {
            console.log('❌ Failed to reload browser:', reloadError);
          }
        } else {
          console.log(`❌ Build failed with code: ${code}`);
        }
      });
      
    } catch (error) {
      console.log(`❌ Error during hot reload: ${error}`);
    }
  }
}

// CLI usage
if (import.meta.url === `file://${process.argv[1]}`) {
  const watcher = new HotReloadWatcher();
  watcher.start();
}