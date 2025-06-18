/**
 * Test Command Module  
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./TestCommand.cjs'),
    client: {
        // Test command could have client-side test execution components
    },
    
    // Module metadata
    name: 'test',
    version: '1.0.0',
    description: 'Test command system - runs comprehensive test suites',
    
    // Machine-readable component structure
    components: {
        server: ['TestCommand.cjs'],
        client: []
    },
    
    // For automated loading systems
    getBrowserFiles: () => [],
    getServerFiles: () => ['TestCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 Test module initialized for ${environment}`);
        return true;
    }
};