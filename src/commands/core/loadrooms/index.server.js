/**
 * LoadRooms Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const LoadRoomsCommand = require('./LoadRoomsCommand.cjs');

export default {
    command: LoadRoomsCommand,
    name: 'loadRooms',
    version: '1.0.0',
    description: 'Room state management functionality',
    type: 'server',
    commands: ['loadRooms'],
    initialize(environment = 'server') {
        console.log(`📦 LoadRooms module (server) v1.0.0 initialized`);
        return true;
    }
};