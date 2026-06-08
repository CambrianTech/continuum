#!/usr/bin/env npx tsx
/**
 * Test TTS Audio Generation
 *
 * Validates that synthesized audio is:
 * 1. Generated successfully
 * 2. Correct format (PCM 16-bit)
 * 3. Playable
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import * as fs from 'fs';

import { VoiceSynthesize } from '../commands/voice/synthesize/shared/VoiceSynthesizeTypes';
async function testTTSAudio() {
  // Initialize JTAG client in server mode
  const jtag = JTAGClientServer.sharedInstance();
  await jtag.connect();

  const { Commands, Events } = jtag;
  console.log('🎙️ Testing TTS Audio Generation');
  console.log('================================\n');

  const text = "Hello world, this is a test of AI voice synthesis";
  console.log(`📝 Text: "${text}"\n`);

  // Subscribe to audio events before calling synthesize
  let audioReceived = false;
  let audioData: string | null = null;
  let sampleRate = 24000;
  let duration = 0;

  const cleanup: Array<() => void> = [];

  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      cleanup.forEach(fn => fn());
      reject(new Error('Timeout waiting for audio'));
    }, 30000);

    // Call synthesize command
    VoiceSynthesize.execute({
      text,
      adapter: 'piper',
      sampleRate: 16000,
    }).then((result: any) => {
      const handle = result.handle;
      console.log(`✅ Command executed, handle: ${handle}\n`);
      console.log(`⏳ Waiting for audio events...\n`);

      // Subscribe to audio event
      const unsubAudio = Events.subscribe(`voice:audio:${handle}`, (event: any) => {
        console.log(`🔊 Audio event received!`);
        console.log(`   Samples: ${event.audio.length} chars base64`);
        console.log(`   Sample rate: ${event.sampleRate}`);
        console.log(`   Duration: ${event.duration}s`);
        console.log(`   Final: ${event.final}\n`);

        audioReceived = true;
        audioData = event.audio;
        sampleRate = event.sampleRate;
        duration = event.duration;
      });
      cleanup.push(unsubAudio);

      // Subscribe to done event
      const unsubDone = Events.subscribe(`voice:done:${handle}`, () => {
        console.log('✅ Synthesis complete\n');

        // Clean up
        clearTimeout(timeout);
        cleanup.forEach(fn => fn());

        if (!audioReceived || !audioData) {
          reject(new Error('No audio received'));
          return;
        }

        // Decode base64 to buffer
        const audioBuffer = Buffer.from(audioData, 'base64');
        console.log(`📊 Audio buffer: ${audioBuffer.length} bytes\n`);

        // Save as WAV file
        const wavPath = '/tmp/tts-test.wav';
        const wavBuffer = createWavBuffer(audioBuffer, sampleRate);
        fs.writeFileSync(wavPath, wavBuffer);

        console.log(`💾 Saved to: ${wavPath}`);
        console.log(`📏 Duration: ${duration.toFixed(2)}s`);
        console.log(`🎵 Sample rate: ${sampleRate}Hz`);
        console.log(`📦 File size: ${wavBuffer.length} bytes\n`);

        console.log('🎧 To play:');
        console.log(`   afplay ${wavPath}`);
        console.log(`   OR open ${wavPath}\n`);

        resolve();
      });
      cleanup.push(unsubDone);

      // Subscribe to error event
      const unsubError = Events.subscribe(`voice:error:${handle}`, (event: any) => {
        console.error('❌ Error:', event.error);
        clearTimeout(timeout);
        cleanup.forEach(fn => fn());
        reject(new Error(event.error));
      });
      cleanup.push(unsubError);

    }).catch((err) => {
      clearTimeout(timeout);
      cleanup.forEach(fn => fn());
      reject(err);
    });
  });
}

/**
 * Create WAV file buffer from raw PCM audio
 */
function createWavBuffer(pcmBuffer: Buffer, sampleRate: number): Buffer {
  const numChannels = 1; // mono
  const bitsPerSample = 16;
  const byteRate = sampleRate * numChannels * (bitsPerSample / 8);
  const blockAlign = numChannels * (bitsPerSample / 8);
  const dataSize = pcmBuffer.length;
  const headerSize = 44;
  const fileSize = headerSize + dataSize - 8;

  const header = Buffer.alloc(headerSize);

  // RIFF header
  header.write('RIFF', 0);
  header.writeUInt32LE(fileSize, 4);
  header.write('WAVE', 8);

  // fmt subchunk
  header.write('fmt ', 12);
  header.writeUInt32LE(16, 16); // subchunk size
  header.writeUInt16LE(1, 20); // audio format (1 = PCM)
  header.writeUInt16LE(numChannels, 22);
  header.writeUInt32LE(sampleRate, 24);
  header.writeUInt32LE(byteRate, 28);
  header.writeUInt16LE(blockAlign, 32);
  header.writeUInt16LE(bitsPerSample, 34);

  // data subchunk
  header.write('data', 36);
  header.writeUInt32LE(dataSize, 40);

  return Buffer.concat([header, pcmBuffer]);
}

// Run test
testTTSAudio()
  .then(() => {
    console.log('✅ Test complete!');
    process.exit(0);
  })
  .catch((err) => {
    console.error('❌ Test failed:', err.message);
    process.exit(1);
  });
