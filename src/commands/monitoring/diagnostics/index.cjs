/**
 * Diagnostics Command Module
 * Clean combined export - easily readable by machine dynamically
 */

module.exports = {
    server: require('./DiagnosticsCommand.cjs'),
    client: {
        // Diagnostics could have client-side browser diagnostic components
    },
    
    // Module metadata
    name: 'diagnostics',
    version: '1.0.0',
    description: 'Diagnostics command system - system health and validation',
    
    // Machine-readable component structure
    components: {
        server: ['DiagnosticsCommand.cjs'],
        client: []
    },
    
    // For automated loading systems
    getBrowserFiles: () => [],
    getServerFiles: () => ['DiagnosticsCommand.cjs'],
    
    // Module initialization
    initialize: (environment = 'server') => {
        console.log(`📦 Diagnostics module initialized for ${environment}`);
        return true;
    }
};