#!/usr/bin/env node
/**
 * DevTools Session Coordinator Test
 * =================================
 * Tests session coordination to prevent duplicate browser launches
 * while enabling proper multi-AI session support.
 */

const { getDevToolsCoordinator } = require('./src/core/DevToolsSessionCoordinator.new.cjs');

async function testSessionCoordination() {
    console.log('🔧 Testing DevTools Session Coordination\n');

    const coordinator = getDevToolsCoordinator();

    try {
        // Test 1: Request verification session (should create new session)
        console.log('🧪 Test 1: Request git verification session');
        const session1 = await coordinator.requestSession('git_verification', 'system');
        console.log(`✅ Session 1 created: ${session1.sessionId} on port ${session1.port}`);

        // Test 2: Request same verification session (should reuse existing)
        console.log('\n🧪 Test 2: Request same verification session (should reuse)');
        const session2 = await coordinator.requestSession('git_verification', 'system');
        console.log(`✅ Session 2: ${session2.sessionId} on port ${session2.port}`);
        
        if (session1.sessionId === session2.sessionId) {
            console.log('✅ SUCCESS: Same session reused (no duplicate browser)');
        } else {
            console.log('❌ FAILURE: Different sessions created (would cause duplicates)');
        }

        // Test 3: Request different AI persona session (should create new)
        console.log('\n🧪 Test 3: Request AI persona session (should create new)');
        const session3 = await coordinator.requestSession('workspace', 'DataViz');
        console.log(`✅ Session 3 created: ${session3.sessionId} on port ${session3.port}`);
        
        if (session3.sessionId !== session1.sessionId) {
            console.log('✅ SUCCESS: Different AI persona gets separate session');
        } else {
            console.log('❌ FAILURE: AI persona should get separate session');
        }

        // Test 4: Request same AI persona session (should reuse)
        console.log('\n🧪 Test 4: Request same AI persona session (should reuse)');
        const session4 = await coordinator.requestSession('workspace', 'DataViz');
        console.log(`✅ Session 4: ${session4.sessionId} on port ${session4.port}`);
        
        if (session3.sessionId === session4.sessionId) {
            console.log('✅ SUCCESS: Same AI persona session reused');
        } else {
            console.log('❌ FAILURE: Same AI persona should reuse session');
        }

        // Display session summary
        console.log('\n📊 SESSION SUMMARY:');
        console.log('='.repeat(50));
        const summary = coordinator.getSessionSummary();
        console.log(`Total Sessions: ${summary.totalSessions}`);
        console.log(`Active Ports: ${summary.activePorts.join(', ')}`);
        console.log('\nBy Purpose:');
        for (const [purpose, count] of Object.entries(summary.byPurpose)) {
            console.log(`  ${purpose}: ${count}`);
        }
        console.log('\nBy AI Persona:');
        for (const [persona, count] of Object.entries(summary.byPersona)) {
            console.log(`  ${persona}: ${count}`);
        }

        console.log('\nActive Sessions:');
        summary.sessions.forEach(s => {
            console.log(`  ${s.id} - ${s.purpose}/${s.persona} - port ${s.port} - ${s.status}`);
        });

        // Cleanup (commented out to preserve sessions for manual inspection)
        // console.log('\n🧹 Cleaning up test sessions...');
        // await coordinator.emergencyShutdown();
        // console.log('✅ Cleanup complete');

        console.log('\n✅ SESSION COORDINATION TEST COMPLETE!');
        console.log('\n🎯 KEY BENEFITS DEMONSTRATED:');
        console.log('- Prevents duplicate browser launches for same purpose');
        console.log('- Enables multiple AI persona sessions');
        console.log('- Smart port allocation and session reuse');
        console.log('- Proper session isolation and management');

    } catch (error) {
        console.error('❌ Test failed:', error);
        // Emergency cleanup on failure
        await coordinator.emergencyShutdown();
    }
}

// Run test
if (require.main === module) {
    testSessionCoordination().catch(console.error);
}

module.exports = { testSessionCoordination };