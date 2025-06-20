/**
 * CI Command Module - Server Export
 * GitHub Actions integration with modular command delegation
 */

const CICommand = require('./CICommand.cjs');

export default {
    command: CICommand,
    name: 'ci',
    version: '1.0.0',
    description: 'GitHub Actions integration with automated issue tracking',
    type: 'server',
    commands: ['ci'],
    initialize(environment = 'server') {
        console.log(`🚀 CI module v1.0.0 initialized for ${environment}`);
        console.log('🔗 GitHub Actions integration ready');
        console.log('📋 Automated issue tracking enabled');
        return true;
    }
};