/**
 * Screenshot Module - Server Entry Point
 * Self-contained server-side functionality
 */

import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Dynamic server-side loading (CommonJS module)
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ScreenshotCommand = require('./ScreenshotCommand.cjs');

// Module metadata
const packageInfo = JSON.parse(readFileSync(join(__dirname, 'package.json'), 'utf8'));

export default {
    // Server command
    command: ScreenshotCommand,
    
    // Module info
    name: packageInfo.name,
    version: packageInfo.version,
    description: packageInfo.description,
    
    // Server-specific metadata
    type: 'server',
    commands: ['screenshot'],
    
    // Initialization
    initialize(environment = 'server') {
        console.log(`📦 Screenshot module (server) v${packageInfo.version} initialized`);
        return true;
    }
};