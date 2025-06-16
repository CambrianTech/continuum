/**
 * Help Command Module
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./HelpCommand.cjs'),
    client: {
        // Help command is server-only for now, but client could have help UI
    },
    
    // Module metadata
    name: 'help',
    version: '1.0.0',
    description: 'Help command system - displays usage and command information',
    
    // Machine-readable component structure
    components: {
        server: ['HelpCommand.cjs'],
        client: []
    },
    
    // For automated loading systems
    getBrowserFiles: () => [],
    getServerFiles: () => ['HelpCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 Help module initialized for ${environment}`);
        return true;
    }
};