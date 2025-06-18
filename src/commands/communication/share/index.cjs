/**
 * Share Command Module
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./ShareCommand.cjs'),
    client: {
        // Share could have client-side sharing UI and integrations
    },
    
    // Module metadata
    name: 'share',
    version: '1.0.0',
    description: 'Share command system - content sharing and distribution',
    
    // Machine-readable component structure
    components: {
        server: ['ShareCommand.cjs'],
        client: []
    },
    
    // For automated loading systems
    getBrowserFiles: () => [],
    getServerFiles: () => ['ShareCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 Share module initialized for ${environment}`);
        return true;
    }
};