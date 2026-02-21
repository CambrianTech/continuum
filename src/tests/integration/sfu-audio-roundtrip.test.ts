/**
 * SFU Audio Channel E2E Test
 *
 * Tests that TTS audio flows through the ACTUAL SFU broadcast channel:
 *   1. Join call as human participant (via WebSocket to call_server)
 *   2. Join call as AI participant (via WebSocket)
 *   3. Inject TTS audio for AI via IPC (voiceSpeakInCall)
 *   4. Human receives binary frames on WebSocket
 *   5. Parse sender_id from new SFU wire format: [0x01][id_len][id][PCM16]
 *   6. Accumulate per-sender audio
 *   7. Feed accumulated audio to STT via IPC
 *   8. Verify transcription matches input text
 *
 * This proves audio goes through: TTS → mixer ring buffer → pull_all_audio()
 *   → audio_tx broadcast → WebSocket SFU wire format → browser parsing → STT
 *
 * Run with: npx tsx tests/integration/sfu-audio-roundtrip.test.ts
 */

import WebSocket from 'ws';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../workers/continuum-core/bindings/RustCoreIPC';
import { CALL_SERVER_URL } from '../../shared/AudioConstants';
import * as fs from 'fs';

const FRAME_KIND_AUDIO = 0x01;
const CALL_ID = `sfu-test-${Date.now()}`;
const HUMAN_USER_ID = 'sfu-test-human';
const AI_USER_ID = 'sfu-test-ai';
const TEST_PHRASE = 'The quick brown fox jumps over the lazy dog';

interface ReceivedAudioFrame {
	senderId: string;
	samples: Int16Array;
}

function parseAudioFrame(data: Buffer): ReceivedAudioFrame | null {
	if (data.length < 3) return null;

	const frameKind = data[0];
	if (frameKind !== FRAME_KIND_AUDIO) return null;

	// SFU wire format: [0x01][sender_id_len: u8][sender_id: UTF-8][PCM16 i16 LE]
	const senderIdLen = data[1];
	const senderIdStart = 2;
	if (data.length < senderIdStart + senderIdLen + 2) return null;

	const senderId = data.subarray(senderIdStart, senderIdStart + senderIdLen).toString('utf-8');
	const audioStart = senderIdStart + senderIdLen;
	const audioBytes = data.subarray(audioStart);

	// Convert LE bytes to Int16Array (must be aligned)
	const aligned = Buffer.alloc(audioBytes.length);
	audioBytes.copy(aligned);
	const samples = new Int16Array(aligned.buffer, aligned.byteOffset, audioBytes.length / 2);

	return { senderId, samples };
}

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

			resolve({
				ws,
				audioFrames,
				close: () => {
					ws.send(JSON.stringify({ type: 'Leave' }));
					ws.close();
				},
			});
		});

		ws.on('message', (data: Buffer) => {
			if (data instanceof Buffer || data instanceof Uint8Array) {
				const buf = Buffer.from(data);
				const frame = parseAudioFrame(buf);
				if (frame) {
					audioFrames.push(frame);
				}
			}
		});

		ws.on('error', reject);

		setTimeout(() => reject(new Error('WebSocket connect timeout')), 5000);
	});
}

function sleep(ms: number): Promise<void> {
	return new Promise(r => setTimeout(r, ms));
}

async function main() {
	console.log('=== SFU Audio Channel E2E Test ===\n');

	const socketPath = getContinuumCoreSocketPath();
	if (!fs.existsSync(socketPath)) {
		console.error(`❌ Socket not found: ${socketPath}`);
		console.error('   Start the system first: npm start');
		process.exit(1);
	}

	// Step 1: Connect IPC client
	const ipc = new RustCoreIPCClient(socketPath);
	await ipc.connect();
	console.log('✅ Connected to continuum-core IPC\n');

	// Step 2: Join call as human participant (will receive audio)
	console.log('📞 Joining call as human participant...');
	const human = await joinCall(HUMAN_USER_ID, 'Test Human', false);
	console.log('   ✅ Human joined\n');

	// Step 3: Join call as AI participant (will speak)
	console.log('🤖 Joining call as AI participant...');
	const ai = await joinCall(AI_USER_ID, 'Test AI', true);
	console.log('   ✅ AI joined\n');

	// Step 4: Give audio loop time to start
	await sleep(200);

	// Step 5: Inject TTS audio for AI via speak_in_call
	console.log(`🔊 Speaking: "${TEST_PHRASE}"`);
	const ttsStart = performance.now();
	try {
		const speakResult = await ipc.voiceSpeakInCall(CALL_ID, AI_USER_ID, TEST_PHRASE, undefined, 'kokoro');
		const ttsMs = performance.now() - ttsStart;
		console.log(`   ✅ TTS: ${speakResult.numSamples} samples, ${speakResult.durationMs}ms audio, adapter=${speakResult.adapter} (took ${ttsMs.toFixed(0)}ms)\n`);
	} catch (e) {
		console.error(`   ❌ voiceSpeakInCall failed: ${(e as Error).message}`);
		human.close();
		ai.close();
		ipc.disconnect();
		process.exit(1);
	}

	// Step 6: Wait for audio to flow through mixer → broadcast → WebSocket
	// AI ring buffer gets pulled by tick() at 50Hz (every 20ms)
	// For ~3.5s of audio, need ~175 ticks ≈ 3.5 seconds + margin
	console.log('⏳ Waiting for audio to flow through SFU channel...');
	await sleep(5000);

	// Step 7: Analyze received audio frames
	const frames = human.audioFrames;
	console.log(`\n📊 Received ${frames.length} audio frames on human's WebSocket`);

	if (frames.length === 0) {
		console.error('❌ NO AUDIO FRAMES RECEIVED — SFU channel is broken!');
		human.close();
		ai.close();
		ipc.disconnect();
		process.exit(1);
	}

	// Group by sender
	const bySender = new Map<string, ReceivedAudioFrame[]>();
	for (const f of frames) {
		const arr = bySender.get(f.senderId) || [];
		arr.push(f);
		bySender.set(f.senderId, arr);
	}

	console.log(`   Senders: ${[...bySender.keys()].join(', ')}`);
	for (const [senderId, senderFrames] of bySender) {
		const totalSamples = senderFrames.reduce((s, f) => s + f.samples.length, 0);
		console.log(`   - "${senderId}": ${senderFrames.length} frames, ${totalSamples} samples (${(totalSamples / 16000).toFixed(1)}s)`);
	}

	// Step 8: Get the AI's audio frames
	const aiFrames = bySender.get(AI_USER_ID);
	if (!aiFrames || aiFrames.length === 0) {
		console.error(`❌ No audio frames from sender "${AI_USER_ID}"`);
		console.error(`   Available senders: ${[...bySender.keys()].join(', ')}`);
		human.close();
		ai.close();
		ipc.disconnect();
		process.exit(1);
	}

	console.log(`\n✅ Received ${aiFrames.length} frames from AI (sender_id="${AI_USER_ID}")`);

	// Step 9: Concatenate all AI audio frames into one buffer
	const totalSamples = aiFrames.reduce((s, f) => s + f.samples.length, 0);
	const allAudio = new Int16Array(totalSamples);
	let offset = 0;
	for (const f of aiFrames) {
		allAudio.set(f.samples, offset);
		offset += f.samples.length;
	}
	console.log(`   Concatenated: ${totalSamples} samples (${(totalSamples / 16000).toFixed(1)}s)\n`);

	// Step 10: Feed to STT via IPC
	console.log('📝 Transcribing received audio via Whisper STT...');
	const audioBase64 = Buffer.from(allAudio.buffer).toString('base64');
	const sttStart = performance.now();
	const sttResponse = await (ipc as any).request({
		command: 'voice/transcribe',
		audio: audioBase64,
		language: 'en',
	});
	const sttMs = performance.now() - sttStart;

	if (!sttResponse.success) {
		console.error(`   ❌ STT failed: ${sttResponse.error}`);
		human.close();
		ai.close();
		ipc.disconnect();
		process.exit(1);
	}

	const transcription = sttResponse.result?.text?.trim() || '';
	const confidence = sttResponse.result?.confidence || 0;
	console.log(`   ✅ STT: "${transcription}" (confidence: ${confidence.toFixed(2)}, took ${sttMs.toFixed(0)}ms)\n`);

	// Step 11: Compare
	const inputLower = TEST_PHRASE.toLowerCase().replace(/[.,!?]/g, '');
	const transLower = transcription.toLowerCase().replace(/[.,!?]/g, '');
	const match = transLower.includes(inputLower) || inputLower.includes(transLower) ||
		wordOverlap(inputLower, transLower) >= 0.5;

	console.log('=== RESULT ===');
	console.log(`Input:          "${TEST_PHRASE}"`);
	console.log(`Transcription:  "${transcription}"`);
	console.log(`Word overlap:   ${(wordOverlap(inputLower, transLower) * 100).toFixed(0)}%`);
	console.log(`Verdict:        ${match ? '✅ PASS — Audio flows through SFU channel and is recognizable by STT' : '❌ FAIL — Transcription does not match input'}`);

	// Cleanup
	human.close();
	ai.close();
	ipc.disconnect();

	if (!match) process.exit(1);
}

function wordOverlap(a: string, b: string): number {
	const wordsA = a.split(/\s+/).filter(w => w.length > 0);
	const wordsB = new Set(b.split(/\s+/).filter(w => w.length > 0));
	if (wordsA.length === 0) return 0;
	return wordsA.filter(w => wordsB.has(w)).length / wordsA.length;
}

main().catch(e => {
	console.error('Fatal error:', e);
	process.exit(1);
});
