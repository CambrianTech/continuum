/**
 * ValidateCode Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ValidateCodeCommand = require('./ValidateCodeCommand.cjs');

export default {
    command: ValidateCodeCommand,
    name: 'validateCode',
    version: '1.0.0',
    description: 'Code validation and analysis',
    type: 'server',
    commands: ['validateCode'],
    initialize(environment = 'server') {
        console.log(`📦 ValidateCode module (server) v1.0.0 initialized`);
        return true;
    }
};