#!/usr/bin/env node
/**
 * Watch for file changes and auto-increment version
 * Ensures we never have cache issues from stale code
 */

import fs from 'fs';
import path from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

let isIncrementing = false;

// Increment version function
function incrementVersion() {
    if (isIncrementing) return;
    isIncrementing = true;
    
    try {
        // Run the increment script
        const result = spawn('node', ['increment-version.js'], { 
            cwd: __dirname,
            stdio: 'pipe'
        });
        
        result.stdout.on('data', (data) => {
            console.log(`📈 ${data.toString().trim()}`);
        });
        
        result.on('close', () => {
            isIncrementing = false;
        });
    } catch (error) {
        console.error('❌ Version increment failed:', error);
        isIncrementing = false;
    }
}

// Watch for file changes
function watchForChanges() {
    const watchDirs = [
        'src/',
        'packages/',
        'launch.ts'
    ];
    
    const extensions = ['.ts', '.js', '.json'];
    
    console.log('👀 Watching for code changes to auto-increment version...');
    
    for (const dir of watchDirs) {
        if (!fs.existsSync(dir)) continue;
        
        fs.watch(dir, { recursive: true }, (eventType, filename) => {
            if (!filename) return;
            
            // Check if it's a file we care about
            const ext = path.extname(filename);
            if (!extensions.includes(ext)) return;
            
            // Skip package.json to avoid infinite loop
            if (filename.endsWith('package.json')) return;
            
            // Skip node_modules and dist
            if (filename.includes('node_modules') || filename.includes('dist/')) return;
            
            console.log(`🔄 Code change detected: ${filename}`);
            
            // Debounce rapid changes
            setTimeout(() => {
                incrementVersion();
            }, 500);
        });
    }
}

// Start watching
watchForChanges();

// Keep process alive
process.on('SIGINT', () => {
    console.log('\n👋 Version watcher stopped');
    process.exit(0);
});