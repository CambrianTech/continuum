/**
 * Docs Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const DocsCommand = require('./DocsCommand.cjs');

export default {
    command: DocsCommand,
    name: 'docs',
    version: '1.0.0',
    description: 'Generate dynamic documentation from help system',
    type: 'server',
    commands: ['docs'],
    initialize(environment = 'server') {
        console.log(`📖 Docs module (server) v1.0.0 initialized`);
        return true;
    }
};