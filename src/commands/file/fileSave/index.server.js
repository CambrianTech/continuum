/**
 * FileSave Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const FileSaveCommand = require('./FileSaveCommand.cjs');

export default {
    command: FileSaveCommand,
    name: 'fileSave',
    version: '1.0.0',
    description: 'File saving functionality',
    type: 'server',
    commands: ['fileSave'],
    initialize(environment = 'server') {
        console.log(`📦 FileSave module (server) v1.0.0 initialized`);
        return true;
    }
};