/**
 * SFU Noise Benchmark — E2E Audio Channel Under Background Noise
 *
 * Tests STT adapter robustness when TTS speech is mixed with ambient noise
 * through the ACTUAL SFU broadcast channel:
 *
 *   1. Connect IPC client + list available STT adapters
 *   2. Join call: human (receiver), AI (speaker)
 *   3. For each noise type:
 *      a. Generate noise via voice/test-audio-generate
 *      b. Add ambient source + inject noise via voice/ambient-inject
 *      c. Speak test phrase via voice/speak-in-call
 *      d. Collect mixed audio frames on human's WebSocket
 *      e. Transcribe with each STT adapter via voice/transcribe-with-adapter
 *      f. Record word accuracy
 *      g. Remove ambient source
 *   4. Print results matrix
 *
 * This proves: TTS + ambient noise → mixer → SFU broadcast → WebSocket → STT
 *
 * Run with: npx tsx tests/integration/sfu-noise-benchmark.test.ts
 */

import WebSocket from 'ws';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../workers/continuum-core/bindings/RustCoreIPC';
import { CALL_SERVER_URL } from '../../shared/AudioConstants';
import type { SttAdapterInfo } from '../../workers/continuum-core/bindings/modules/voice';
import * as fs from 'fs';

// ============================================================================
// Constants
// ============================================================================

const FRAME_KIND_AUDIO = 0x01;
const CALL_ID = `sfu-noise-bench-${Date.now()}`;
const HUMAN_USER_ID = 'noise-bench-human';
const AI_USER_ID = 'noise-bench-ai';
const TEST_PHRASE = 'The quick brown fox jumps over the lazy dog';

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

// SNR is controlled by scaling the ambient audio amplitude before injection.
// These scale factors map to approximate SNR levels relative to normal TTS volume.
// Lower scale = higher SNR (speech dominates), higher scale = lower SNR (noise dominates)
const AMPLITUDE_SCALES = [
	{ label: '20dB', scale: 0.1 },   // Speech 20dB louder than noise
	{ label: '10dB', scale: 0.32 },  // Speech 10dB louder
	{ label: '5dB', scale: 0.56 },   // Speech 5dB louder
	{ label: '0dB', scale: 1.0 },    // Equal volume
];

// ============================================================================
// Audio frame parsing (same as sfu-audio-roundtrip)
// ============================================================================

interface ReceivedAudioFrame {
	senderId: string;
	samples: Int16Array;
}

function parseAudioFrame(data: Buffer): ReceivedAudioFrame | null {
	if (data.length < 3) return null;
	if (data[0] !== FRAME_KIND_AUDIO) return null;

	const senderIdLen = data[1];
	const senderIdStart = 2;
	if (data.length < senderIdStart + senderIdLen + 2) return null;

	const senderId = data.subarray(senderIdStart, senderIdStart + senderIdLen).toString('utf-8');
	const audioStart = senderIdStart + senderIdLen;
	const audioBytes = data.subarray(audioStart);

	const aligned = Buffer.alloc(audioBytes.length);
	audioBytes.copy(aligned);
	const samples = new Int16Array(aligned.buffer, aligned.byteOffset, audioBytes.length / 2);

	return { senderId, samples };
}

// ============================================================================
// WebSocket helpers
// ============================================================================

function joinCall(userId: string, displayName: string, isAi: boolean): Promise<{
	ws: WebSocket;
	audioFrames: ReceivedAudioFrame[];
	close: () => void;
}> {
	return new Promise((resolve, reject) => {
		const ws = new WebSocket(CALL_SERVER_URL);
		const audioFrames: ReceivedAudioFrame[] = [];

		ws.on('open', () => {
			ws.send(JSON.stringify({
				type: 'Join',
				call_id: CALL_ID,
				user_id: userId,
				display_name: displayName,
				is_ai: isAi,
			}));
			resolve({ ws, audioFrames, close: () => { ws.send(JSON.stringify({ type: 'Leave' })); ws.close(); } });
		});

		ws.on('message', (data: Buffer) => {
			if (data instanceof Buffer || data instanceof Uint8Array) {
				const frame = parseAudioFrame(Buffer.from(data));
				if (frame) audioFrames.push(frame);
			}
		});

		ws.on('error', reject);
		setTimeout(() => reject(new Error('WebSocket connect timeout')), 5000);
	});
}

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

/**
 * Scale i16 PCM samples by a float factor.
 * Decodes base64, scales each sample, returns scaled base64.
 */
function scaleAudioBase64(audioB64: string, scale: number): number[] {
	const buf = Buffer.from(audioB64, 'base64');
	const samples: number[] = [];
	for (let i = 0; i < buf.length; i += 2) {
		const sample = buf.readInt16LE(i);
		const scaled = Math.round(sample * scale);
		samples.push(Math.max(-32767, Math.min(32767, scaled)));
	}
	return samples;
}

// ============================================================================
// Main benchmark
// ============================================================================

interface BenchmarkRow {
	noiseLabel: string;
	snrLabel: string;
	adapter: string;
	wordAccuracy: number;
	transcription: string;
}

async function main() {
	console.log('='.repeat(80));
	console.log('  SFU Noise Benchmark — E2E Audio Channel Under Background Noise');
	console.log('='.repeat(80));
	console.log();

	const socketPath = getContinuumCoreSocketPath();
	if (!fs.existsSync(socketPath)) {
		console.error(`Socket not found: ${socketPath}`);
		console.error('Start the system first: npm start');
		process.exit(1);
	}

	const ipc = new RustCoreIPCClient(socketPath);
	await ipc.connect();
	console.log('Connected to continuum-core IPC\n');

	// Step 1: List STT adapters
	const sttInfo = await ipc.voiceSttList();
	const adapters = sttInfo.adapters
		.filter((a: SttAdapterInfo) => a.name !== 'stub')
		.map((a: SttAdapterInfo) => a.name);

	console.log(`STT adapters: ${adapters.join(', ')} (active: ${sttInfo.active})`);

	if (adapters.length === 0) {
		console.error('No STT adapters available. Exiting.');
		ipc.disconnect();
		process.exit(1);
	}

	// Step 2: Join call
	console.log('\nJoining call...');
	const human = await joinCall(HUMAN_USER_ID, 'Benchmark Human', false);
	const ai = await joinCall(AI_USER_ID, 'Benchmark AI', true);
	console.log('  Human + AI joined\n');
	await sleep(200);

	const results: BenchmarkRow[] = [];

	// Step 3: Clean baseline (no noise)
	console.log('--- Clean baseline (no noise) ---');
	{
		// Clear any leftover frames
		human.audioFrames.length = 0;

		await ipc.voiceSpeakInCall(CALL_ID, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');
		await sleep(5000);

		const frames = human.audioFrames.filter(f => f.senderId === AI_USER_ID);
		if (frames.length === 0) {
			console.error('No audio frames received from AI — SFU channel broken!');
			human.close(); ai.close(); ipc.disconnect();
			process.exit(1);
		}

		const totalSamples = frames.reduce((s, f) => s + f.samples.length, 0);
		const allAudio = new Int16Array(totalSamples);
		let offset = 0;
		for (const f of frames) { allAudio.set(f.samples, offset); offset += f.samples.length; }

		const audioB64 = Buffer.from(allAudio.buffer).toString('base64');

		for (const adapter of adapters) {
			try {
				const result = await ipc.voiceTranscribeWithAdapter(audioB64, adapter, 'en');
				const accuracy = wordOverlap(TEST_PHRASE, result.text);
				console.log(`  ${adapter}: ${(accuracy * 100).toFixed(0)}% — "${result.text}"`);
				results.push({ noiseLabel: 'clean', snrLabel: 'inf', adapter, wordAccuracy: accuracy, transcription: result.text });
			} catch (e) {
				console.log(`  ${adapter}: ERROR — ${(e as Error).message}`);
			}
		}

		// Wait between tests to let audio buffers drain
		await sleep(1000);
		human.audioFrames.length = 0;
	}

	// Step 4: Test each noise type
	for (const noise of NOISE_TYPES) {
		console.log(`\n--- Noise: ${noise.label} ---`);

		// Generate noise audio via IPC
		const noiseResult = await ipc.voiceTestAudioGenerate(
			noise.name,
			noise.durationMs,
			noise.params,
		);
		console.log(`  Generated ${noiseResult.samples} noise samples (${noiseResult.duration_ms}ms)`);

		for (const snr of AMPLITUDE_SCALES) {
			console.log(`  SNR ~${snr.label}:`);

			// Scale noise to target SNR
			const scaledNoiseSamples = scaleAudioBase64(noiseResult.audio, snr.scale);

			// Add ambient source
			const ambientHandle = await ipc.voiceAmbientAdd(CALL_ID, `noise-${noise.label}`);

			// Inject scaled noise as ambient
			// Split into chunks to avoid overwhelming the mixer (2000 samples per chunk)
			const chunkSize = 2000;
			for (let i = 0; i < scaledNoiseSamples.length; i += chunkSize) {
				const chunk = scaledNoiseSamples.slice(i, i + chunkSize);
				await ipc.voiceAmbientInject(CALL_ID, ambientHandle.handle, chunk);
			}

			// Clear frames and speak
			human.audioFrames.length = 0;

			await ipc.voiceSpeakInCall(CALL_ID, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');

			// Wait for audio to flow through mixer
			await sleep(5000);

			// Collect mixed audio frames
			const frames = human.audioFrames.filter(f => f.senderId === AI_USER_ID);
			const totalSamples = frames.reduce((s, f) => s + f.samples.length, 0);

			if (totalSamples === 0) {
				console.log(`    No audio frames received, skipping`);
				await ipc.voiceAmbientRemove(CALL_ID, ambientHandle.handle);
				continue;
			}

			const allAudio = new Int16Array(totalSamples);
			let offset = 0;
			for (const f of frames) { allAudio.set(f.samples, offset); offset += f.samples.length; }

			const audioB64 = Buffer.from(allAudio.buffer).toString('base64');

			// Transcribe with each adapter
			for (const adapter of adapters) {
				try {
					const result = await ipc.voiceTranscribeWithAdapter(audioB64, adapter, 'en');
					const accuracy = wordOverlap(TEST_PHRASE, result.text);
					console.log(`    ${adapter}: ${(accuracy * 100).toFixed(0)}% — "${result.text}"`);
					results.push({
						noiseLabel: noise.label,
						snrLabel: snr.label,
						adapter,
						wordAccuracy: accuracy,
						transcription: result.text,
					});
				} catch (e) {
					console.log(`    ${adapter}: ERROR — ${(e as Error).message}`);
				}
			}

			// Remove ambient source
			await ipc.voiceAmbientRemove(CALL_ID, ambientHandle.handle);
			await sleep(500);
			human.audioFrames.length = 0;
		}
	}

	// Step 5: Print results matrix
	console.log('\n' + '='.repeat(90));
	console.log('RESULTS MATRIX — Word Accuracy (%) through SFU Channel');
	console.log('='.repeat(90));

	// Header
	const adapterCols = adapters.map(a => a.padStart(12)).join(' | ');
	console.log(`${'Noise'.padEnd(12)} ${'SNR'.padStart(6)} | ${adapterCols}`);
	console.log('-'.repeat(90));

	// Clean row
	{
		const cols = adapters.map(adapter => {
			const r = results.find(r => r.noiseLabel === 'clean' && r.adapter === adapter);
			return r ? `${(r.wordAccuracy * 100).toFixed(0)}%`.padStart(12) : '         N/A';
		}).join(' | ');
		console.log(`${'clean'.padEnd(12)} ${'inf'.padStart(6)} | ${cols}`);
	}

	// Noise rows
	for (const noise of NOISE_TYPES) {
		for (const snr of AMPLITUDE_SCALES) {
			const cols = adapters.map(adapter => {
				const r = results.find(r =>
					r.noiseLabel === noise.label &&
					r.snrLabel === snr.label &&
					r.adapter === adapter
				);
				return r ? `${(r.wordAccuracy * 100).toFixed(0)}%`.padStart(12) : '         N/A';
			}).join(' | ');
			console.log(`${noise.label.padEnd(12)} ${snr.label.padStart(6)} | ${cols}`);
		}
	}

	console.log('='.repeat(90));
	console.log(`\nTotal data points: ${results.length}`);

	// Cleanup
	human.close();
	ai.close();
	ipc.disconnect();
}

main().catch(e => {
	console.error('Fatal error:', e);
	process.exit(1);
});
