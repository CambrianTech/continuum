/**
 * Issues Command Module - Server Export
 * AI-driven GitHub issue management with dashboard integration
 */

const IssuesCommand = require('./IssuesCommand.cjs');

export default {
    command: IssuesCommand,
    name: 'issues',
    version: '1.0.0',
    description: 'AI-driven GitHub issue management with multi-agent collaboration',
    type: 'server',
    commands: ['issues'],
    initialize(environment = 'server') {
        console.log(`🎯 Issues module v1.0.0 initialized for ${environment}`);
        return true;
    }
};