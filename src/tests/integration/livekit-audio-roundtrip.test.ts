/**
 * LiveKit Audio Roundtrip — Isolated E2E Tests
 *
 * Tests audio pipeline through LiveKit WITHOUT needing the full system (npm start).
 * Prerequisites: LiveKit server running + continuum-core IPC socket available.
 *
 * Test layers (in order of isolation):
 *
 *   Layer 1: TTS → STT direct (no transport) — baseline
 *   Layer 2: TTS + noise → STT (mixing, no transport) — STT robustness
 *   Layer 3: TTS → LiveKit publish → verify room tracks — publish path
 *   Layer 4: TTS → LiveKit → STT listener → transcription buffer — full roundtrip
 *   Layer 5: TTS + ambient noise → LiveKit → STT listener → verify — noise through LiveKit
 *
 * Run with: npx tsx tests/integration/livekit-audio-roundtrip.test.ts [--layer=N]
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../workers/continuum-core/bindings/RustCoreIPC';
import { LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET, LIVEKIT_PORT, AUDIO_SAMPLE_RATE } from '../../shared/AudioConstants';
import type { TranscriptionEntry, VoiceParticipant } from '../../workers/continuum-core/bindings/modules/voice';
import * as fs from 'fs';
import * as http from 'http';
import * as crypto from 'crypto';

// =============================================================================
// Configuration
// =============================================================================

const TEST_PHRASE = 'The quick brown fox jumps over the lazy dog';
const SHORT_PHRASE = 'Hello world';
const CALL_ID = `livekit-test-${Date.now()}`;
const AI_USER_ID = 'livekit-test-ai';
const HUMAN_USER_ID = 'livekit-test-human';

const NOISE_TYPES = [
	{ name: 'crowd', label: 'crowd-5', durationMs: 6000, params: { voice_count: 5 } },
	{ name: 'factory', label: 'factory', durationMs: 6000 },
	{ name: 'gunfire', label: 'gunfire', durationMs: 6000, params: { shots_per_second: 3 } },
	{ name: 'explosion', label: 'explosion', durationMs: 6000 },
	{ name: 'siren', label: 'siren', durationMs: 6000 },
	{ name: 'music', label: 'music', durationMs: 6000 },
	{ name: 'wind', label: 'wind', durationMs: 6000 },
	{ name: 'rain', label: 'rain', durationMs: 6000 },
];

const AMPLITUDE_SCALES = [
	{ label: '20dB', scale: 0.1 },
	{ label: '10dB', scale: 0.32 },
	{ label: '5dB', scale: 0.56 },
	{ label: '0dB', scale: 1.0 },
];

// =============================================================================
// Utilities
// =============================================================================

function sleep(ms: number): Promise<void> {
	return new Promise(r => setTimeout(r, ms));
}

function wordOverlap(expected: string, actual: string): number {
	const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9\s]/g, '').split(/\s+/).filter(w => w.length > 0);
	const wordsA = norm(expected);
	const wordsB = new Set(norm(actual));
	if (wordsA.length === 0) return 0;
	return wordsA.filter(w => wordsB.has(w)).length / wordsA.length;
}

/** Mix i16 PCM speech buffer with base64-encoded noise (additive, clamped) */
function mixAudio(speechBuf: Buffer, noiseB64: string, noiseScale: number): Buffer {
	const speechCopy = Buffer.alloc(speechBuf.length);
	speechBuf.copy(speechCopy);
	const speechSamples = new Int16Array(speechCopy.buffer, speechCopy.byteOffset, speechCopy.length / 2);

	const noiseDec = Buffer.from(noiseB64, 'base64');
	const noiseCopy = Buffer.alloc(noiseDec.length);
	noiseDec.copy(noiseCopy);
	const noiseSamples = new Int16Array(noiseCopy.buffer, noiseCopy.byteOffset, noiseCopy.length / 2);

	const length = Math.max(speechSamples.length, noiseSamples.length);
	const mixed = new Int16Array(length);

	for (let i = 0; i < length; i++) {
		const s = i < speechSamples.length ? speechSamples[i] : 0;
		const n = i < noiseSamples.length ? Math.round(noiseSamples[i] * noiseScale) : 0;
		mixed[i] = Math.max(-32768, Math.min(32767, s + n));
	}

	return Buffer.from(mixed.buffer);
}

/** Scale noise samples (from base64) to target amplitude */
function scaleNoiseToSamples(audioB64: string, scale: number): number[] {
	const buf = Buffer.from(audioB64, 'base64');
	const samples: number[] = [];
	for (let i = 0; i < buf.length; i += 2) {
		const sample = buf.readInt16LE(i);
		const scaled = Math.round(sample * scale);
		samples.push(Math.max(-32768, Math.min(32767, scaled)));
	}
	return samples;
}

/** Check if LiveKit server is reachable at the expected port */
async function checkLiveKitServer(): Promise<boolean> {
	return new Promise((resolve) => {
		const req = http.request(
			{ hostname: '127.0.0.1', port: LIVEKIT_PORT, path: '/', method: 'GET', timeout: 2000 },
			(res) => { resolve(true); res.resume(); }
		);
		req.on('error', () => resolve(false));
		req.on('timeout', () => { req.destroy(); resolve(false); });
		req.end();
	});
}

/** Query LiveKit room participants via livekit-server-sdk */
async function getLiveKitRoomParticipants(roomName: string): Promise<any[]> {
	const { RoomServiceClient } = await import('livekit-server-sdk');
	const client = new RoomServiceClient(
		`http://127.0.0.1:${LIVEKIT_PORT}`,
		LIVEKIT_API_KEY,
		LIVEKIT_API_SECRET,
	);
	const participants = await client.listParticipants(roomName);
	return participants;
}

/** Query LiveKit rooms list */
async function getLiveKitRooms(): Promise<any[]> {
	const { RoomServiceClient } = await import('livekit-server-sdk');
	const client = new RoomServiceClient(
		`http://127.0.0.1:${LIVEKIT_PORT}`,
		LIVEKIT_API_KEY,
		LIVEKIT_API_SECRET,
	);
	const rooms = await client.listRooms();
	return rooms;
}

// =============================================================================
// Test Results Tracking
// =============================================================================

interface TestResult {
	layer: number;
	name: string;
	passed: boolean;
	detail: string;
	durationMs: number;
}

const results: TestResult[] = [];

function record(layer: number, name: string, passed: boolean, detail: string, durationMs: number) {
	results.push({ layer, name, passed, detail, durationMs });
	const icon = passed ? '✅' : '❌';
	console.log(`  ${icon} ${name}: ${detail} (${durationMs.toFixed(0)}ms)`);
}

// =============================================================================
// Layer 1: TTS → STT Direct (No Transport)
// =============================================================================

async function layer1_tts_stt_direct(ipc: RustCoreIPCClient) {
	console.log('\n' + '='.repeat(70));
	console.log('  LAYER 1: TTS → STT Direct (no transport — baseline)');
	console.log('='.repeat(70));

	for (const phrase of [SHORT_PHRASE, TEST_PHRASE]) {
		const start = performance.now();

		// TTS
		const tts = await ipc.voiceSynthesize(phrase, undefined, 'kokoro');
		const ttsMs = performance.now() - start;
		console.log(`  TTS: ${tts.numSamples} samples, ${tts.durationMs}ms audio (took ${ttsMs.toFixed(0)}ms)`);

		// STT
		const audioB64 = tts.audio.toString('base64');
		const sttStart = performance.now();
		const stt = await ipc.voiceTranscribeWithAdapter(audioB64, 'whisper', 'en');
		const sttMs = performance.now() - sttStart;

		const accuracy = wordOverlap(phrase, stt.text);
		const totalMs = performance.now() - start;

		record(1, `"${phrase}"`, accuracy >= 0.5,
			`→ "${stt.text}" (${(accuracy * 100).toFixed(0)}% match, TTS=${ttsMs.toFixed(0)}ms, STT=${sttMs.toFixed(0)}ms)`,
			totalMs);
	}
}

// =============================================================================
// Layer 2: TTS + Noise → STT (Mixing, No Transport)
// =============================================================================

interface NoiseResult {
	noiseLabel: string;
	snrLabel: string;
	accuracy: number;
	transcription: string;
}

async function layer2_tts_noise_stt(ipc: RustCoreIPCClient) {
	console.log('\n' + '='.repeat(70));
	console.log('  LAYER 2: TTS + Noise → STT (local mixing — STT robustness)');
	console.log('='.repeat(70));

	// First: TTS the test phrase
	const tts = await ipc.voiceSynthesize(TEST_PHRASE, undefined, 'kokoro');
	console.log(`  TTS baseline: ${tts.numSamples} samples, ${tts.durationMs}ms\n`);

	const noiseResults: NoiseResult[] = [];

	for (const noise of NOISE_TYPES) {
		console.log(`  --- Noise: ${noise.label} ---`);
		const noiseAudio = await ipc.voiceTestAudioGenerate(noise.name, noise.durationMs, noise.params);

		for (const snr of AMPLITUDE_SCALES) {
			const start = performance.now();

			// Mix speech + noise locally
			const mixed = mixAudio(tts.audio, noiseAudio.audio, snr.scale);
			const mixedB64 = mixed.toString('base64');

			// STT on mixed audio
			const stt = await ipc.voiceTranscribeWithAdapter(mixedB64, 'whisper', 'en');
			const accuracy = wordOverlap(TEST_PHRASE, stt.text);
			const ms = performance.now() - start;

			noiseResults.push({ noiseLabel: noise.label, snrLabel: snr.label, accuracy, transcription: stt.text });

			record(2, `${noise.label} @ ${snr.label}`, accuracy >= 0.3,
				`→ "${stt.text}" (${(accuracy * 100).toFixed(0)}%)`, ms);
		}
	}

	// Print results matrix
	printNoiseMatrix(noiseResults);
}

function printNoiseMatrix(results: NoiseResult[]) {
	console.log('\n' + '='.repeat(70));
	console.log('  NOISE MATRIX — Word Accuracy (%)');
	console.log('='.repeat(70));

	const header = ['Noise', ...AMPLITUDE_SCALES.map(s => s.label)].map(s => s.padStart(10)).join(' | ');
	console.log(`  ${header}`);
	console.log('  ' + '-'.repeat(60));

	for (const noise of NOISE_TYPES) {
		const cols = AMPLITUDE_SCALES.map(snr => {
			const r = results.find(r => r.noiseLabel === noise.label && r.snrLabel === snr.label);
			return r ? `${(r.accuracy * 100).toFixed(0)}%` : 'N/A';
		}).map(s => s.padStart(10)).join(' | ');
		console.log(`  ${noise.label.padStart(10)} | ${cols}`);
	}
}

// =============================================================================
// Layer 3: TTS → LiveKit Publish → Verify Room Tracks
// =============================================================================

async function layer3_livekit_publish(ipc: RustCoreIPCClient) {
	console.log('\n' + '='.repeat(70));
	console.log('  LAYER 3: TTS → LiveKit Publish → Verify Room Tracks');
	console.log('='.repeat(70));

	// Check LiveKit server is up
	const lkUp = await checkLiveKitServer();
	if (!lkUp) {
		record(3, 'LiveKit server reachable', false, 'LiveKit server not responding on port ' + LIVEKIT_PORT, 0);
		return;
	}
	record(3, 'LiveKit server reachable', true, `port ${LIVEKIT_PORT}`, 0);

	// Speak in call — this creates a LiveKitAgent and publishes to the room
	const start = performance.now();
	const speakResult = await ipc.voiceSpeakInCall(CALL_ID, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');
	const speakMs = performance.now() - start;

	record(3, 'speak-in-call succeeded', true,
		`${speakResult.numSamples} samples, ${speakResult.durationMs}ms audio`, speakMs);

	// Wait for LiveKit to register the participant
	await sleep(2000);

	// Verify room exists and has the agent with audio track
	try {
		const participants = await getLiveKitRoomParticipants(CALL_ID);
		const agentParticipant = participants.find((p: any) => p.identity === AI_USER_ID);

		if (!agentParticipant) {
			record(3, 'Agent in LiveKit room', false,
				`Agent "${AI_USER_ID}" not found. Participants: ${participants.map((p: any) => p.identity).join(', ')}`, 0);
			return;
		}

		record(3, 'Agent in LiveKit room', true,
			`identity="${agentParticipant.identity}", name="${agentParticipant.name}"`, 0);

		// Check published tracks (LiveKit protobuf: type 0=AUDIO 1=VIDEO, source 1=CAMERA 2=MICROPHONE)
		const tracks = agentParticipant.tracks || [];
		const audioTracks = tracks.filter((t: any) => t.type === 0 || t.source === 2);
		const videoTracks = tracks.filter((t: any) => t.type === 1 || t.source === 1);

		record(3, 'Audio track published', audioTracks.length > 0,
			`${audioTracks.length} audio track(s), ${videoTracks.length} video track(s)`, 0);

	} catch (e) {
		record(3, 'LiveKit room API query', false, (e as Error).message, 0);
	}
}

// =============================================================================
// Layer 4: Full LiveKit Roundtrip — TTS → LiveKit → STT Listener → Transcription
// =============================================================================

async function layer4_full_roundtrip(ipc: RustCoreIPCClient) {
	console.log('\n' + '='.repeat(70));
	console.log('  LAYER 4: Full LiveKit Roundtrip — TTS → LiveKit → STT → Transcription');
	console.log('='.repeat(70));

	const callId = crypto.randomUUID();

	// Step 1: Register session (spawns STT listener)
	const participants: VoiceParticipant[] = [
		{ user_id: HUMAN_USER_ID, display_name: 'Test Human', participant_type: 'human', expertise: [], is_audio_native: false },
	];

	const start = performance.now();
	await ipc.voiceRegisterSession(callId, callId, participants);
	record(4, 'Session registered (STT listener spawned)', true, `callId=${callId.slice(0, 8)}`, performance.now() - start);

	// Wait for STT listener to connect
	await sleep(2000);

	// Step 2: Drain any stale transcriptions
	await ipc.voicePollTranscriptions(callId);

	// Step 3: Speak in the same call (publishes TTS to LiveKit room)
	const speakStart = performance.now();
	await ipc.voiceSpeakInCall(callId, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');
	record(4, 'TTS published to LiveKit', true, `"${TEST_PHRASE}"`, performance.now() - speakStart);

	// Step 4: Wait for STT listener to process (VAD → STT → buffer)
	// VAD needs silence after speech to detect sentence boundary.
	// The TTS audio is finite, so after it finishes, there's silence → VAD triggers.
	console.log('  ⏳ Waiting for STT listener to process (VAD + STT)...');

	let transcription: TranscriptionEntry | null = null;
	const maxWaitMs = 30000;
	const pollIntervalMs = 1000;
	const waitStart = performance.now();

	while (performance.now() - waitStart < maxWaitMs) {
		await sleep(pollIntervalMs);
		const pollResult = await ipc.voicePollTranscriptions(callId);
		if (pollResult.count > 0) {
			transcription = pollResult.transcriptions[0];
			break;
		}
	}

	const waitMs = performance.now() - waitStart;

	if (!transcription) {
		record(4, 'STT listener produced transcription', false,
			`No transcription after ${(waitMs / 1000).toFixed(1)}s. STT listener may not have subscribed to agent's audio track.`, waitMs);
		return;
	}

	const accuracy = wordOverlap(TEST_PHRASE, transcription.text);
	record(4, 'Full LiveKit roundtrip', accuracy >= 0.3,
		`→ "${transcription.text}" (${(accuracy * 100).toFixed(0)}% match, speaker=${transcription.speaker_id.slice(0, 16)})`,
		waitMs);
}

// =============================================================================
// Layer 5: TTS + Ambient Noise → LiveKit → STT Listener → Verify
// =============================================================================

async function layer5_noise_roundtrip(ipc: RustCoreIPCClient) {
	console.log('\n' + '='.repeat(70));
	console.log('  LAYER 5: TTS + Ambient Noise → LiveKit → STT Listener → Verify');
	console.log('='.repeat(70));

	const callId = crypto.randomUUID();

	// Register session + STT listener
	const participants: VoiceParticipant[] = [
		{ user_id: HUMAN_USER_ID, display_name: 'Test Human', participant_type: 'human', expertise: [], is_audio_native: false },
	];
	await ipc.voiceRegisterSession(callId, callId, participants);
	await sleep(2000);

	// Test subset of noise types for speed (gunfire is the user's favorite test case)
	const testNoises = NOISE_TYPES.filter(n => ['gunfire', 'crowd-5', 'music'].includes(n.label));
	const testSnrs = [{ label: '10dB', scale: 0.32 }];

	for (const noise of testNoises) {
		for (const snr of testSnrs) {
			console.log(`\n  --- ${noise.label} @ ${snr.label} ---`);
			const start = performance.now();

			// Generate noise
			const noiseAudio = await ipc.voiceTestAudioGenerate(noise.name, noise.durationMs, noise.params);
			const scaledSamples = scaleNoiseToSamples(noiseAudio.audio, snr.scale);

			// Add ambient source to the call
			const ambient = await ipc.voiceAmbientAdd(callId, `test-${noise.label}`);

			// Inject noise in small chunks to avoid IPC timeout (max ~1s of audio per call)
			const chunkSize = 16000; // 1 second at 16kHz
			const maxChunks = 4; // Only inject first 4 seconds to keep test fast
			let chunksInjected = 0;
			for (let i = 0; i < scaledSamples.length && chunksInjected < maxChunks; i += chunkSize) {
				const chunk = scaledSamples.slice(i, i + chunkSize);
				await ipc.voiceAmbientInject(callId, ambient.handle, chunk);
				chunksInjected++;
			}

			// Drain stale transcriptions
			await ipc.voicePollTranscriptions(callId);

			// Speak with noise playing
			await ipc.voiceSpeakInCall(callId, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');

			// Wait for transcription
			let transcription: TranscriptionEntry | null = null;
			const waitStart = performance.now();
			const maxWait = 20000;

			while (performance.now() - waitStart < maxWait) {
				await sleep(1000);
				const poll = await ipc.voicePollTranscriptions(callId);
				if (poll.count > 0) {
					// Find the transcription from the AI (not the noise)
					transcription = poll.transcriptions.find(t => t.speaker_id === AI_USER_ID) || poll.transcriptions[0];
					break;
				}
			}

			// Remove ambient
			await ipc.voiceAmbientRemove(callId, ambient.handle);

			const totalMs = performance.now() - start;

			if (!transcription) {
				record(5, `${noise.label} @ ${snr.label} roundtrip`, false,
					`No transcription after ${((performance.now() - waitStart) / 1000).toFixed(1)}s`, totalMs);
			} else {
				const accuracy = wordOverlap(TEST_PHRASE, transcription.text);
				record(5, `${noise.label} @ ${snr.label} roundtrip`, accuracy >= 0.2,
					`→ "${transcription.text}" (${(accuracy * 100).toFixed(0)}%)`, totalMs);
			}
		}
	}
}

// =============================================================================
// Main
// =============================================================================

async function main() {
	const args = process.argv.slice(2);
	const layerArg = args.find(a => a.startsWith('--layer='));
	const targetLayer = layerArg ? parseInt(layerArg.split('=')[1]) : 0; // 0 = all

	console.log('='.repeat(70));
	console.log('  LiveKit Audio Roundtrip — Isolated E2E Tests');
	console.log('='.repeat(70));
	console.log(`  Target: ${targetLayer > 0 ? `Layer ${targetLayer} only` : 'All layers'}`);

	// Prerequisites
	const socketPath = getContinuumCoreSocketPath();
	if (!fs.existsSync(socketPath)) {
		console.error(`\n❌ IPC socket not found: ${socketPath}`);
		console.error('   Start the system first: npm start (or just start continuum-core worker)');
		process.exit(1);
	}

	const ipc = new RustCoreIPCClient(socketPath);
	await ipc.connect();
	console.log(`\n  ✅ Connected to continuum-core IPC`);

	// Warmup TTS (first call loads ONNX model)
	console.log('  ⏳ Warming up Kokoro TTS...');
	const warmStart = performance.now();
	await ipc.voiceSynthesize('warmup', undefined, 'kokoro');
	console.log(`  ✅ TTS warmup: ${(performance.now() - warmStart).toFixed(0)}ms`);

	// Run requested layers
	const totalStart = performance.now();

	if (targetLayer === 0 || targetLayer === 1) await layer1_tts_stt_direct(ipc);
	if (targetLayer === 0 || targetLayer === 2) await layer2_tts_noise_stt(ipc);
	if (targetLayer === 0 || targetLayer === 3) await layer3_livekit_publish(ipc);
	if (targetLayer === 0 || targetLayer === 4) await layer4_full_roundtrip(ipc);
	if (targetLayer === 0 || targetLayer === 5) await layer5_noise_roundtrip(ipc);

	const totalMs = performance.now() - totalStart;

	// Summary
	console.log('\n' + '='.repeat(70));
	console.log('  SUMMARY');
	console.log('='.repeat(70));

	const passed = results.filter(r => r.passed);
	const failed = results.filter(r => !r.passed);

	console.log(`\n  ${passed.length} passed, ${failed.length} failed out of ${results.length} tests (${(totalMs / 1000).toFixed(1)}s)\n`);

	if (failed.length > 0) {
		console.log('  FAILURES:');
		for (const f of failed) {
			console.log(`    ❌ L${f.layer}: ${f.name} — ${f.detail}`);
		}
	}

	// Group by layer
	for (let l = 1; l <= 5; l++) {
		const layerTests = results.filter(r => r.layer === l);
		if (layerTests.length === 0) continue;
		const layerPassed = layerTests.filter(r => r.passed).length;
		const icon = layerPassed === layerTests.length ? '✅' : layerPassed > 0 ? '⚠️' : '❌';
		console.log(`  ${icon} Layer ${l}: ${layerPassed}/${layerTests.length} passed`);
	}

	ipc.disconnect();

	if (failed.length > 0) process.exit(1);
}

main().catch(e => {
	console.error('Fatal error:', e);
	process.exit(1);
});
