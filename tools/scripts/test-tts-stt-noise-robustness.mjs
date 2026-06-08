#!/usr/bin/env node
/**
 * TTS → STT Noise Robustness Test
 * Tests speech recognition accuracy with varying levels of background noise
 */

import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PROTO_PATH = join(__dirname, '../workers/streaming-core/proto/voice.proto');

console.log('🔊 TTS → STT Noise Robustness Test');
console.log('===================================\n');

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

const testPhrases = [
  "Hello world this is a test",
  "The quick brown fox jumps over the lazy dog",
  "Testing speech recognition with background noise",
];

// Add white noise to audio samples
function addWhiteNoise(samples, snrDb) {
  const snrLinear = Math.pow(10, snrDb / 20);

  // Calculate signal power
  let signalPower = 0;
  for (let i = 0; i < samples.length; i++) {
    signalPower += samples[i] * samples[i];
  }
  signalPower /= samples.length;

  // Calculate noise power needed for target SNR
  const noisePower = signalPower / (snrLinear * snrLinear);
  const noiseStdDev = Math.sqrt(noisePower);

  // Add Gaussian white noise
  const noisySamples = new Int16Array(samples.length);
  for (let i = 0; i < samples.length; i++) {
    // Box-Muller transform for Gaussian noise
    const u1 = Math.random();
    const u2 = Math.random();
    const noise = Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(2.0 * Math.PI * u2) * noiseStdDev;

    // Add noise and clamp to int16 range
    const noisy = samples[i] + noise;
    noisySamples[i] = Math.max(-32768, Math.min(32767, Math.round(noisy)));
  }

  return noisySamples;
}

// Test at different SNR levels
const snrLevels = [
  { db: Infinity, label: 'Clean (no noise)' },
  { db: 30, label: '30 dB SNR (quiet room)' },
  { db: 20, label: '20 dB SNR (normal conversation)' },
  { db: 10, label: '10 dB SNR (noisy environment)' },
  { db: 5, label: '5 dB SNR (very noisy)' },
  { db: 0, label: '0 dB SNR (extremely noisy)' },
];

let currentPhrase = 0;
let currentSnr = 0;
const results = [];

function testNext() {
  if (currentPhrase >= testPhrases.length) {
    printResults();
    process.exit(0);
    return;
  }

  const text = testPhrases[currentPhrase];
  const snr = snrLevels[currentSnr];

  console.log(`\n📝 Testing: "${text}"`);
  console.log(`   Noise level: ${snr.label}`);

  // Synthesize clean audio
  client.Synthesize(
    {
      text,
      voice: '',
      adapter: 'piper',
      speed: 1.0,
      sample_rate: 16000,
    },
    (err, ttsResponse) => {
      if (err) {
        console.error('❌ TTS Error:', err.message);
        process.exit(1);
      }

      // Decode audio
      const audioBuffer = Buffer.from(ttsResponse.audio);
      const samples = new Int16Array(audioBuffer.buffer, audioBuffer.byteOffset, audioBuffer.byteLength / 2);

      // Add noise (if not clean)
      const noisySamples = snr.db === Infinity ? samples : addWhiteNoise(samples, snr.db);

      // Re-encode to bytes
      const noisyBuffer = Buffer.from(noisySamples.buffer, noisySamples.byteOffset, noisySamples.byteLength);

      // Transcribe
      client.Transcribe(
        {
          audio: noisyBuffer,
          language: 'en',
          model: 'base',
        },
        (err, sttResponse) => {
          if (err) {
            console.error('❌ STT Error:', err.message);
            process.exit(1);
          }

          const transcribed = sttResponse.text.toLowerCase().trim();
          const original = text.toLowerCase().trim();
          const match = transcribed === original;

          // Calculate word accuracy
          const originalWords = original.split(/\s+/);
          const transcribedWords = transcribed.split(/\s+/);
          let correctWords = 0;
          for (const word of originalWords) {
            if (transcribedWords.includes(word)) {
              correctWords++;
            }
          }
          const wordAccuracy = (correctWords / originalWords.length) * 100;

          console.log(`   Transcribed: "${sttResponse.text}"`);
          console.log(`   Match: ${match ? '✅' : '❌'} (${wordAccuracy.toFixed(0)}% word accuracy)`);

          results.push({
            text,
            snr: snr.db,
            snrLabel: snr.label,
            transcribed: sttResponse.text,
            match,
            wordAccuracy
          });

          // Move to next test
          currentSnr++;
          if (currentSnr >= snrLevels.length) {
            currentSnr = 0;
            currentPhrase++;
          }

          setTimeout(testNext, 100);
        }
      );
    }
  );
}

function printResults() {
  console.log('\n\n📊 Noise Robustness Results');
  console.log('===========================\n');

  // Group by SNR level
  const bySnr = {};
  for (const result of results) {
    if (!bySnr[result.snrLabel]) {
      bySnr[result.snrLabel] = [];
    }
    bySnr[result.snrLabel].push(result);
  }

  for (const snr of snrLevels) {
    const tests = bySnr[snr.label] || [];
    if (tests.length === 0) continue;

    const avgAccuracy = tests.reduce((sum, t) => sum + t.wordAccuracy, 0) / tests.length;
    const exactMatches = tests.filter(t => t.match).length;

    console.log(`${snr.label}:`);
    console.log(`  Exact matches: ${exactMatches}/${tests.length}`);
    console.log(`  Avg word accuracy: ${avgAccuracy.toFixed(1)}%`);

    if (avgAccuracy < 50) {
      console.log(`  ⚠️  Poor accuracy - speech unintelligible at this noise level`);
    } else if (avgAccuracy < 80) {
      console.log(`  ⚠️  Degraded accuracy - some words lost`);
    } else if (avgAccuracy < 100) {
      console.log(`  ✅ Good accuracy - mostly understandable`);
    } else {
      console.log(`  ✅ Perfect accuracy`);
    }
    console.log();
  }

  // Overall summary
  const cleanTests = bySnr[snrLevels[0].label] || [];
  const cleanAccuracy = cleanTests.reduce((sum, t) => sum + t.wordAccuracy, 0) / cleanTests.length;

  if (cleanAccuracy < 100) {
    console.log('⚠️  WARNING: Clean audio not 100% accurate - TTS may have issues');
  } else {
    console.log('✅ Clean audio: 100% accurate');
  }

  // Find minimum SNR for >80% accuracy
  let minUsableSNR = null;
  for (let i = snrLevels.length - 1; i >= 0; i--) {
    const tests = bySnr[snrLevels[i].label] || [];
    const avgAccuracy = tests.reduce((sum, t) => sum + t.wordAccuracy, 0) / tests.length;
    if (avgAccuracy >= 80) {
      minUsableSNR = snrLevels[i];
      break;
    }
  }

  if (minUsableSNR) {
    console.log(`\n📈 Minimum usable SNR: ${minUsableSNR.label}`);
    console.log('   (>80% word accuracy threshold)');
  } else {
    console.log('\n⚠️  No SNR level achieved >80% accuracy');
  }
}

// Start testing
testNext();
