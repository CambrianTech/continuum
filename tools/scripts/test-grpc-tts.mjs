#!/usr/bin/env node
/**
 * Direct gRPC TTS Test
 * Calls the Rust gRPC service directly and saves audio to WAV
 */

import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { writeFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PROTO_PATH = join(__dirname, '../workers/streaming-core/proto/voice.proto');

console.log('🎙️ Direct gRPC TTS Test');
console.log('======================\n');

// Load proto
const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
  keepCase: true,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});

const protoDescriptor = grpc.loadPackageDefinition(packageDefinition);
const VoiceService = protoDescriptor.voice.VoiceService;

// Create client
const client = new VoiceService(
  '127.0.0.1:50052',
  grpc.credentials.createInsecure()
);

const text = "Hello world, this is a direct gRPC test of AI voice synthesis";
console.log(`📝 Text: "${text}"\n`);

// Call Synthesize
console.log('⏳ Calling gRPC Synthesize...\n');

client.Synthesize(
  {
    text,
    voice: '',
    adapter: 'piper',
    speed: 1.0,
    sample_rate: 16000,
  },
  (err, response) => {
    if (err) {
      console.error('❌ Error:', err.message);
      process.exit(1);
    }

    console.log('✅ Synthesis complete!\n');
    console.log(`📊 Response:`);
    console.log(`   Sample rate: ${response.sample_rate}`);
    console.log(`   Duration: ${response.duration_ms}ms`);
    console.log(`   Adapter: ${response.adapter}`);
    console.log(`   Audio data: ${response.audio.length} bytes (base64)\n`);

    // Decode base64 audio
    const audioBuffer = Buffer.from(response.audio, 'base64');
    console.log(`📦 Decoded audio: ${audioBuffer.length} bytes PCM\n`);

    // Create WAV file
    const wavBuffer = createWavBuffer(audioBuffer, response.sample_rate);
    const wavPath = '/tmp/grpc-tts-test.wav';
    writeFileSync(wavPath, wavBuffer);

    console.log(`💾 Saved to: ${wavPath}`);
    console.log(`📏 Duration: ${(response.duration_ms / 1000).toFixed(2)}s`);
    console.log(`🎵 Sample rate: ${response.sample_rate}Hz`);
    console.log(`📦 WAV file size: ${wavBuffer.length} bytes\n`);

    console.log('🎧 To play:');
    console.log(`   afplay ${wavPath}\n`);

    console.log('✅ Test complete!');
    process.exit(0);
  }
);

function createWavBuffer(pcmBuffer, sampleRate) {
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
