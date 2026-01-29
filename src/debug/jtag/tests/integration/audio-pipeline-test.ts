/**
 * Audio Pipeline Integration Test
 *
 * Tests the full audio pipeline by:
 * 1. Synthesizing known text with TTS
 * 2. Transcribing it back with STT
 * 3. Verifying the transcription matches
 *
 * Run with: npx tsx tests/integration/audio-pipeline-test.ts
 */

import { Commands } from '../../system/core/shared/Commands';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';

import { VoiceSynthesize } from '../../commands/voice/synthesize/shared/VoiceSynthesizeTypes';
import { VoiceTranscribe } from '../../commands/voice/transcribe/shared/VoiceTranscribeTypes';
const TEST_PHRASES = [
  'Hello world',
  'The quick brown fox',
  'Testing one two three',
];

async function testAudioPipeline() {
  console.log('=== Audio Pipeline Integration Test ===\n');

  // Connect to JTAG
  const client = new JTAGClient();
  await client.connect();
  console.log('✓ Connected to JTAG\n');

  let passed = 0;
  let failed = 0;

  for (const phrase of TEST_PHRASES) {
    console.log(`Testing: "${phrase}"`);

    try {
      // Step 1: Synthesize speech
      console.log('  1. Synthesizing with TTS...');
      const synthResult = await VoiceSynthesize.execute({
        text: phrase,
        adapter: 'piper',
      });

      if (!synthResult.success) {
        throw new Error(`TTS failed: ${synthResult.error}`);
      }

      console.log(`     ✓ TTS returned handle: ${synthResult.handle}`);
      console.log(`     ✓ Sample rate: ${synthResult.sampleRate}Hz`);

      // Wait for audio event
      const audioData = await waitForAudioEvent(synthResult.handle, 10000);
      console.log(`     ✓ Received ${audioData.length} bytes of audio`);

      // Step 2: Transcribe the audio back
      console.log('  2. Transcribing with STT...');
      const transcribeResult = await VoiceTranscribe.execute({
        audio: audioData.toString('base64'),
        format: 'pcm16',
      });

      if (!transcribeResult.success) {
        throw new Error(`STT failed: ${transcribeResult.error}`);
      }

      const transcribed = transcribeResult.text?.toLowerCase().trim() || '';
      const expected = phrase.toLowerCase().trim();

      console.log(`     ✓ Transcribed: "${transcribed}"`);
      console.log(`     ✓ Expected:    "${expected}"`);

      // Step 3: Compare
      const similarity = calculateSimilarity(expected, transcribed);
      console.log(`     ✓ Similarity: ${(similarity * 100).toFixed(1)}%`);

      if (similarity > 0.6) {
        console.log('  ✅ PASSED\n');
        passed++;
      } else {
        console.log('  ❌ FAILED - transcription mismatch\n');
        failed++;
      }

    } catch (error) {
      console.log(`  ❌ FAILED - ${error}\n`);
      failed++;
    }
  }

  console.log('=== Results ===');
  console.log(`Passed: ${passed}/${TEST_PHRASES.length}`);
  console.log(`Failed: ${failed}/${TEST_PHRASES.length}`);

  await client.disconnect();

  process.exit(failed > 0 ? 1 : 0);
}

async function waitForAudioEvent(handle: string, timeoutMs: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error(`Timeout waiting for audio event ${handle}`));
    }, timeoutMs);

    const { Events } = require('../../system/core/shared/Events');

    const unsub = Events.subscribe(`voice:audio:${handle}`, (data: any) => {
      clearTimeout(timeout);
      unsub();

      if (data.audio) {
        resolve(Buffer.from(data.audio, 'base64'));
      } else {
        reject(new Error('No audio data in event'));
      }
    });
  });
}

function calculateSimilarity(a: string, b: string): number {
  const wordsA = a.split(/\s+/);
  const wordsB = b.split(/\s+/);

  let matches = 0;
  for (const word of wordsA) {
    if (wordsB.includes(word)) matches++;
  }

  return matches / Math.max(wordsA.length, wordsB.length);
}

testAudioPipeline().catch(console.error);
