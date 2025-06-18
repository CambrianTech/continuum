/**
 * Test Command Module - Server Entry Point
 * Self-contained server-side test functionality
 */

import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const TestCommand = require('./TestCommand.cjs');

export default {
    // Server command
    command: TestCommand,
    
    // Module info
    name: 'test',
    version: '1.0.0',
    description: 'Test command system - runs comprehensive test suites',
    
    // Server-specific metadata
    type: 'server',
    commands: ['test'],
    
    // Initialization
    initialize(environment = 'server') {
        console.log(`📦 Test module (server) v1.0.0 initialized`);
        return true;
    }
};