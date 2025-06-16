/**
 * FindUser Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const FindUserCommand = require('./FindUserCommand.cjs');

export default {
    command: FindUserCommand,
    name: 'findUser',
    version: '1.0.0',
    description: 'Find and select users',
    type: 'server',
    commands: ['findUser'],
    initialize(environment = 'server') {
        console.log(`📦 FindUser module (server) v1.0.0 initialized`);
        return true;
    }
};