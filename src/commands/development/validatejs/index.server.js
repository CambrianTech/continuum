/**
 * ValidateJS Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ValidateJSCommand = require('./ValidateJSCommand.cjs');

export default {
    command: ValidateJSCommand,
    name: 'validateJS',
    version: '1.0.0',
    description: 'JavaScript validation and linting',
    type: 'server',
    commands: ['validateJS'],
    initialize(environment = 'server') {
        console.log(`📦 ValidateJS module (server) v1.0.0 initialized`);
        return true;
    }
};