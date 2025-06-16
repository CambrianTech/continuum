/**
 * Screenshot Command Module
 * Complete screenshot functionality in one proper module
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./ScreenshotCommand.cjs'),
    client: {
        // Only load client files in browser environment to avoid server-side window errors
        get command() {
            if (typeof window !== 'undefined') {
                return require('./ScreenshotCommand.client.js');
            }
            return null;
        },
        get utils() {
            if (typeof window !== 'undefined') {
                return require('./ScreenshotUtils.js');
            }
            return null;
        }
    },
    
    // Module metadata
    name: 'screenshot',
    commandName: 'screenshot', // Explicit command name for matching
    version: '1.0.0',
    description: 'Complete screenshot command system with server and client components',
    
    // Machine-readable component structure
    components: {
        server: ['ScreenshotCommand.cjs'],
        client: ['ScreenshotCommand.client.js', 'ScreenshotUtils.js']
    },
    
    // For automated loading systems
    getBrowserFiles: () => ['ScreenshotCommand.client.js', 'ScreenshotUtils.js'],
    getServerFiles: () => ['ScreenshotCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 Screenshot module initialized for ${environment}`);
        return true;
    }
};