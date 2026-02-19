#!/usr/bin/env node
/**
 * TTS-only test - Generate audio and analyze without STT
 */

import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { writeFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PROTO_PATH = join(__dirname, '../workers/streaming-core/proto/voice.proto');

console.log('🎙️  TTS-Only Test (No STT)');
console.log('=========================\n');

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

const text = "Hello world, this is a test of real speech synthesis";
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
    console.log(`   Sample rate: ${response.sample_rate}Hz`);
    console.log(`   Duration: ${response.duration_ms}ms`);
    console.log(`   Adapter: ${response.adapter}`);
    console.log(`   Audio data: ${response.audio.length} bytes (base64)\n`);

    // Decode base64 audio
    const audioBuffer = Buffer.from(response.audio, 'base64');
    console.log(`📦 Decoded audio: ${audioBuffer.length} bytes PCM\n`);

    // Analyze the audio samples
    const samples = new Int16Array(audioBuffer.buffer, audioBuffer.byteOffset, audioBuffer.byteLength / 2);

    console.log('🔬 Audio Analysis:');
    console.log('==================');

    const nonZero = samples.filter(s => s !== 0).length;
    console.log(`Non-zero samples: ${nonZero}/${samples.length} (${(nonZero/samples.length*100).toFixed(1)}%)`);

    const amplitudes = Array.from(samples).map(Math.abs);
    const maxAmp = Math.max(...amplitudes);
    const avgAmp = amplitudes.reduce((a, b) => a + b, 0) / amplitudes.length;

    console.log(`Max amplitude: ${maxAmp} / 32767 (${(maxAmp/32767*100).toFixed(1)}% of full scale)`);
    console.log(`Avg amplitude: ${avgAmp.toFixed(1)}`);

    // Check for DC offset (all positive or all negative)
    const positive = samples.filter(s => s > 0).length;
    const negative = samples.filter(s => s < 0).length;
    console.log(`Positive samples: ${positive} (${(positive/samples.length*100).toFixed(1)}%)`);
    console.log(`Negative samples: ${negative} (${(negative/samples.length*100).toFixed(1)}%)`);

    // Check zero-crossing rate (speech should be ~0.05-0.15)
    let zeroAcrossings = 0;
    for (let i = 1; i < samples.length; i++) {
      if ((samples[i-1] >= 0 && samples[i] < 0) || (samples[i-1] < 0 && samples[i] >= 0)) {
        zeroAcrossings++;
      }
    }
    const zcr = zeroAcrossings / samples.length;
    console.log(`Zero-crossing rate: ${zcr.toFixed(4)}`);
    console.log(`  (Speech: ~0.05-0.15, Noise: >0.3)\n`);

    // Sample values
    console.log(`First 20 samples: ${Array.from(samples.slice(0, 20)).join(', ')}\n`);

    // Diagnosis
    console.log('🔍 Diagnosis:');
    if (nonZero === 0) {
      console.log('❌ SILENCE (all zeros)');
    } else if (positive === samples.length || negative === samples.length) {
      console.log('❌ DC OFFSET (all samples same sign - this was the old bug)');
    } else if (zcr > 0.3) {
      console.log('⚠️  HIGH NOISE (zero-crossing rate too high)');
    } else if (avgAmp < 100) {
      console.log('⚠️  TOO QUIET (very low amplitude)');
    } else if (zcr >= 0.05 && zcr <= 0.20 && avgAmp > 1000) {
      console.log('✅ LOOKS LIKE REAL SPEECH!');
      console.log('   - Zero-crossing rate in speech range');
      console.log('   - Good amplitude variation');
      console.log('   - Samples cross zero (no DC offset)');
    } else {
      console.log('⚠️  UNCERTAIN - manual verification needed');
    }

    // Create WAV file
    const wavBuffer = createWavBuffer(audioBuffer, response.sample_rate);
    const wavPath = '/tmp/tts-test.wav';
    writeFileSync(wavPath, wavBuffer);

    console.log(`\n💾 Saved to: ${wavPath}`);
    console.log(`🎧 To play: afplay ${wavPath}\n`);

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
