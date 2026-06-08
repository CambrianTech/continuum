#!/usr/bin/env node
/**
 * End-to-End Voice Test
 *
 * Simulates PersonaUser speaking in a voice call:
 * 1. Generate AI response text
 * 2. Synthesize to speech
 * 3. Save audio (simulating sending to WebSocket)
 *
 * This validates the full pipeline before wiring into PersonaUser.
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { writeFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

console.log('🤖 End-to-End: PersonaUser Voice Response');
console.log('==========================================\n');

console.log('📝 Scenario: User asks "What is AI?" in voice call');
console.log('🎯 Goal: Helper AI responds with synthesized speech\n');

// Step 1: Simulate AI response generation
console.log('Step 1: Generate AI response text');
console.log('----------------------------------');

const aiResponse = "AI, or artificial intelligence, is the simulation of human intelligence in machines. " +
  "These systems can learn, reason, and perform tasks that typically require human intelligence.";

console.log(`✅ AI response: "${aiResponse}"`);
console.log(`   Length: ${aiResponse.length} chars\n`);

// Step 2: Synthesize speech
console.log('Step 2: Synthesize speech with TTS');
console.log('-----------------------------------');

// Import gRPC client
const grpc = await import('@grpc/grpc-js');
const protoLoader = await import('@grpc/proto-loader');

const PROTO_PATH = join(__dirname, '../workers/streaming-core/proto/voice.proto');

const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
const VoiceService = protoDescriptor.voice.VoiceService;

const client = new VoiceService(
  '127.0.0.1:50052',
  grpc.credentials.createInsecure()
);

const startTime = Date.now();

client.Synthesize(
  {
    text: aiResponse,
    voice: '',
    adapter: 'piper',
    speed: 1.0,
    sample_rate: 16000,
  },
  (err, response) => {
    if (err) {
      console.error('❌ Synthesis failed:', err.message);
      process.exit(1);
    }

    const elapsed = Date.now() - startTime;

    console.log(`✅ Synthesis complete in ${elapsed}ms`);
    console.log(`   Sample rate: ${response.sample_rate}Hz`);
    console.log(`   Duration: ${response.duration_ms}ms`);
    console.log(`   Adapter: ${response.adapter}`);
    console.log(`   Audio size: ${response.audio.length} bytes (base64)\n`);

    // Step 3: Convert to WebSocket format
    console.log('Step 3: Convert to WebSocket format');
    console.log('------------------------------------');

    const audioBuffer = Buffer.from(response.audio, 'base64');
    console.log(`✅ Decoded: ${audioBuffer.length} bytes PCM`);

    // Convert to i16 array (WebSocket format)
    const audioSamples = new Int16Array(audioBuffer.length / 2);
    for (let i = 0; i < audioSamples.length; i++) {
      audioSamples[i] = audioBuffer.readInt16LE(i * 2);
    }

    console.log(`✅ Converted to i16 array: ${audioSamples.length} samples`);
    console.log(`   Format: Vec<i16> ready for WebSocket\n`);

    // Step 4: Save for testing
    console.log('Step 4: Save audio for verification');
    console.log('-------------------------------------');

    // Create WAV for testing
    const wavBuffer = createWavBuffer(audioBuffer, response.sample_rate);
    const wavPath = '/tmp/persona-voice-e2e.wav';
    writeFileSync(wavPath, wavBuffer);

    console.log(`✅ Saved to: ${wavPath}`);
    console.log(`   Play with: afplay ${wavPath}\n`);

    // Summary
    console.log('📊 Performance Summary');
    console.log('----------------------');
    console.log(`⏱️  Total time: ${elapsed}ms`);
    console.log(`📏 Audio duration: ${(response.duration_ms / 1000).toFixed(2)}s`);
    console.log(`⚡ Realtime factor: ${(elapsed / response.duration_ms).toFixed(2)}x`);
    console.log(`   (Lower is better - 1x means synthesis time = audio duration)\n`);

    if (elapsed < response.duration_ms) {
      console.log('✅ Fast enough for real-time (synthesis faster than playback)');
    } else if (elapsed < response.duration_ms * 2) {
      console.log('⚠️  Borderline for real-time (synthesis ~2x audio duration)');
    } else {
      console.log('❌ Too slow for real-time conversation');
    }

    console.log('\n🎯 Next Step: Wire PersonaUser.respondInCall()');
    console.log('   PersonaUser.respondInCall(text) {');
    console.log('     const voice = getVoiceService();');
    console.log('     const audio = await voice.synthesizeSpeech({ text });');
    console.log('     voiceSession.sendAudio(audio.audioSamples);');
    console.log('   }\n');

    console.log('✅ End-to-end test complete!');
    process.exit(0);
  }
);

function createWavBuffer(pcmBuffer, sampleRate) {
  const numChannels = 1;
  const bitsPerSample = 16;
  const byteRate = sampleRate * numChannels * (bitsPerSample / 8);
  const blockAlign = numChannels * (bitsPerSample / 8);
  const dataSize = pcmBuffer.length;
  const headerSize = 44;
  const fileSize = headerSize + dataSize - 8;

  const header = Buffer.alloc(headerSize);

  header.write('RIFF', 0);
  header.writeUInt32LE(fileSize, 4);
  header.write('WAVE', 8);

  header.write('fmt ', 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(numChannels, 22);
  header.writeUInt32LE(sampleRate, 24);
  header.writeUInt32LE(byteRate, 28);
  header.writeUInt16LE(blockAlign, 32);
  header.writeUInt16LE(bitsPerSample, 34);

  header.write('data', 36);
  header.writeUInt32LE(dataSize, 40);

  return Buffer.concat([header, pcmBuffer]);
}
