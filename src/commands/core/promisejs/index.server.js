/**
 * PromiseJS Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const PromiseJSCommand = require('./PromiseJSCommand.cjs');

export default {
    command: PromiseJSCommand,
    name: 'promiseJS',
    version: '1.0.0',
    description: 'Promise-based JavaScript execution',
    type: 'server',
    commands: ['promiseJS'],
    initialize(environment = 'server') {
        console.log(`📦 PromiseJS module (server) v1.0.0 initialized`);
        return true;
    }
};