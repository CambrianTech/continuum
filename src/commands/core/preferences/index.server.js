/**
 * Preferences Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const PreferencesCommand = require('./PreferencesCommand.cjs');

export default {
    command: PreferencesCommand,
    name: 'preferences',
    version: '1.0.0',
    description: 'User preferences and settings',
    type: 'server',
    commands: ['preferences'],
    initialize(environment = 'server') {
        console.log(`📦 Preferences module (server) v1.0.0 initialized`);
        return true;
    }
};