#!/usr/bin/env node
/**
 * TTS → STT Roundtrip Test
 * Synthesize text, then transcribe it to verify audio quality
 */

import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PROTO_PATH = join(__dirname, '../workers/streaming-core/proto/voice.proto');

console.log('🔄 TTS → STT Roundtrip Test');
console.log('===========================\n');

const originalText = "Hello world this is a test";
console.log(`📝 Original text: "${originalText}"\n`);

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

// Step 1: Synthesize
console.log('Step 1: Synthesize with Piper TTS');
console.log('----------------------------------');

client.Synthesize(
  {
    text: originalText,
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

    console.log(`✅ TTS complete: ${ttsResponse.audio.length} bytes (base64)\n`);

    // Step 2: Transcribe
    console.log('Step 2: Transcribe with Whisper STT');
    console.log('------------------------------------');

    client.Transcribe(
      {
        audio: ttsResponse.audio,
        language: 'en',
        model: 'base',
      },
      (err, sttResponse) => {
        if (err) {
          console.error('❌ STT Error:', err.message);
          process.exit(1);
        }

        console.log(`✅ STT complete\n`);

        // Step 3: Compare
        console.log('📊 Roundtrip Results');
        console.log('====================');
        console.log(`Original:     "${originalText}"`);
        console.log(`Transcribed:  "${sttResponse.text}"`);

        const match = sttResponse.text.toLowerCase().trim() === originalText.toLowerCase().trim();
        console.log(`Exact match:  ${match ? '✅ YES' : '❌ NO'}`);

        // Check for key words
        const hasHello = sttResponse.text.toLowerCase().includes('hello');
        const hasWorld = sttResponse.text.toLowerCase().includes('world');
        const hasTest = sttResponse.text.toLowerCase().includes('test');

        console.log(`\nKey words detected:`);
        console.log(`  "hello": ${hasHello ? '✅' : '❌'}`);
        console.log(`  "world": ${hasWorld ? '✅' : '❌'}`);
        console.log(`  "test":  ${hasTest ? '✅' : '❌'}`);

        // Final verdict
        console.log('\n🔍 Verdict');
        console.log('==========');
        if (hasHello && hasWorld && hasTest) {
          console.log('✅ TTS is producing REAL SPEECH');
          console.log('   Whisper successfully understood the synthesized audio');
        } else if (hasHello || hasWorld) {
          console.log('⚠️  TTS is producing PARTIAL SPEECH');
          console.log('   Some words recognized, quality may be poor');
        } else {
          console.log('❌ TTS is producing STATIC/GARBAGE');
          console.log('   Whisper could not recognize the audio');
        }

        process.exit(0);
      }
    );
  }
);
