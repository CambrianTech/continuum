/**
 * Share Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ShareCommand = require('./ShareCommand.cjs');

export default {
    command: ShareCommand,
    name: 'share',
    version: '1.0.0',
    description: 'Content sharing functionality',
    type: 'server',
    commands: ['share'],
    initialize(environment = 'server') {
        console.log(`📦 Share module (server) v1.0.0 initialized`);
        return true;
    }
};