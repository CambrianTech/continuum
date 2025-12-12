/**
 * Quick Test Client for TrainingWorker
 *
 * Tests the full round-trip: TypeScript → TrainingWorker → LoggerWorker
 */

import { TrainingWorkerClient } from './ipc/training/TrainingWorkerClient.js';

async function testTrainingWorker() {
    console.log('📡 Connecting to TrainingWorker...');

    const client = new TrainingWorkerClient('/tmp/jtag-training-worker.sock');

    try {
        await client.connect();
        console.log('✅ Connected to TrainingWorker');

        // Test ping
        console.log('\n🏓 Testing ping...');
        const pingResult = await client.ping();
        console.log('✅ Ping response:', pingResult);

        // Test export (will create empty file for now)
        console.log('\n📤 Testing export-training...');
        const exportResult = await client.exportSample('/tmp/training-test.jsonl', 10);
        console.log('✅ Export response:', exportResult);
        console.log(`   Exported ${exportResult.examplesExported} examples`);
        console.log(`   Wrote ${exportResult.bytesWritten} bytes`);
        console.log(`   Duration: ${exportResult.durationMs}ms`);

        console.log('\n✅ All tests passed!');

    } catch (error) {
        console.error('❌ Test failed:', error);
        process.exit(1);
    } finally {
        await client.disconnect();
    }
}

testTrainingWorker();
