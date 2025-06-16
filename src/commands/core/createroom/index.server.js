/**
 * CreateRoom Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const CreateRoomCommand = require('./CreateRoomCommand.cjs');

export default {
    command: CreateRoomCommand,
    name: 'createRoom',
    version: '1.0.0',
    description: 'Discord-style room management functionality',
    type: 'server',
    commands: ['createRoom'],
    initialize(environment = 'server') {
        console.log(`📦 CreateRoom module (server) v1.0.0 initialized`);
        return true;
    }
};