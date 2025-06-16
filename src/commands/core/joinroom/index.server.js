/**
 * JoinRoom Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const JoinRoomCommand = require('./JoinRoomCommand.cjs');

export default {
    command: JoinRoomCommand,
    name: 'joinRoom',
    version: '1.0.0',
    description: 'Room subscription functionality',
    type: 'server',
    commands: ['joinRoom'],
    initialize(environment = 'server') {
        console.log(`📦 JoinRoom module (server) v1.0.0 initialized`);
        return true;
    }
};