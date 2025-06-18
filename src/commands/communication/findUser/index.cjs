/**
 * FindUser Command Module
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./FindUserCommand.cjs'),
    client: {
        // FindUser could have client-side user selection UI
    },
    
    // Module metadata
    name: 'findUser',
    version: '1.0.0',
    description: 'FindUser command system - user lookup and selection',
    
    // Machine-readable component structure
    components: {
        server: ['FindUserCommand.cjs'],
        client: []
    },
    
    // For automated loading systems
    getBrowserFiles: () => [],
    getServerFiles: () => ['FindUserCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 FindUser module initialized for ${environment}`);
        return true;
    }
};