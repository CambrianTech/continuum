/**
 * Layer 5: TTS → STT Round-Trip Integration Test
 *
 * Synthesizes known text with Kokoro TTS, then transcribes it back with
 * Whisper STT. Validates that the transcription matches the input.
 * Reports per-stage timing for optimization.
 *
 * Prerequisites:
 *   - continuum-core server running (npm start or start-workers.sh)
 *   - Kokoro model + Whisper model loaded
 *
 * Run with: npx tsx tests/integration/tts-stt-roundtrip.test.ts
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../workers/continuum-core/bindings/RustCoreIPC';
import * as fs from 'fs';

const SOCKET_PATH = getContinuumCoreSocketPath();

interface StageResult {
	stage: string;
	durationMs: number;
	detail: string;
}

interface RoundTripResult {
	phrase: string;
	stages: StageResult[];
	totalMs: number;
	transcription: string;
	match: boolean;
	similarity: number;
}

function assert(condition: boolean, message: string): void {
	if (!condition) throw new Error(`Assertion failed: ${message}`);
}

/**
 * Number words ↔ digits mapping.
 * Whisper often transcribes "one two three" as "1, 2, 3".
 */
const NUMBER_WORDS: Record<string, string> = {
	zero: '0', one: '1', two: '2', three: '3', four: '4',
	five: '5', six: '6', seven: '7', eight: '8', nine: '9',
	ten: '10', eleven: '11', twelve: '12', thirteen: '13',
	fourteen: '14', fifteen: '15', sixteen: '16', seventeen: '17',
	eighteen: '18', nineteen: '19', twenty: '20',
};

// Reverse mapping: digit → word
const DIGIT_WORDS: Record<string, string> = {};
for (const [word, digit] of Object.entries(NUMBER_WORDS)) {
	DIGIT_WORDS[digit] = word;
}

/**
 * Word-level similarity: fraction of input words matched in transcription.
 * Case-insensitive, strips punctuation, handles number-word ↔ digit equivalence.
 */
function wordSimilarity(input: string, transcription: string): number {
	const normalize = (s: string) => {
		const words = s.toLowerCase().replace(/[.,!?;:'"()\-]/g, '').split(/\s+/).filter(w => w.length > 0);
		// Split concatenated digit strings like "12345" into ["1", "2", "3", "4", "5"]
		// Whisper sometimes concatenates numbers without spaces
		const expanded: string[] = [];
		for (const w of words) {
			if (/^\d+$/.test(w) && w.length > 1) {
				expanded.push(...w.split(''));
			} else {
				expanded.push(w);
			}
		}
		return expanded;
	};

	const inputWords = normalize(input);
	const transcriptWords = new Set(normalize(transcription));

	if (inputWords.length === 0) return 0;

	const matchCount = inputWords.filter(w => {
		if (transcriptWords.has(w)) return true;
		// Try number-word ↔ digit equivalence
		if (NUMBER_WORDS[w] && transcriptWords.has(NUMBER_WORDS[w])) return true;
		if (DIGIT_WORDS[w] && transcriptWords.has(DIGIT_WORDS[w])) return true;
		return false;
	}).length;
	return matchCount / inputWords.length;
}

async function testRoundTrip(
	client: RustCoreIPCClient,
	phrase: string,
	voice: string = 'af',
): Promise<RoundTripResult> {
	const stages: StageResult[] = [];
	const totalStart = performance.now();

	// Stage 1: TTS Synthesis
	const ttsStart = performance.now();
	const ttsResult = await client.voiceSynthesize(phrase, voice, 'kokoro');
	const ttsMs = performance.now() - ttsStart;
	stages.push({
		stage: 'TTS (Kokoro)',
		durationMs: ttsMs,
		detail: `${ttsResult.numSamples} samples, ${ttsResult.durationMs}ms audio, ${ttsResult.sampleRate}Hz`,
	});

	// Stage 2: Encode audio for STT (base64 for IPC transport)
	const encodeStart = performance.now();
	const audioBase64 = ttsResult.audio.toString('base64');
	const encodeMs = performance.now() - encodeStart;
	stages.push({
		stage: 'Base64 encode',
		durationMs: encodeMs,
		detail: `${ttsResult.audio.length} bytes → ${audioBase64.length} chars`,
	});

	// Stage 3: STT Transcription (via IPC)
	const sttStart = performance.now();
	// Send raw request since voiceTranscribe isn't in the client yet
	const sttResponse = await (client as any).request({
		command: 'voice/transcribe',
		audio: audioBase64,
		language: 'en',
	});
	const sttMs = performance.now() - sttStart;

	if (!sttResponse.success) {
		stages.push({
			stage: 'STT (Whisper)',
			durationMs: sttMs,
			detail: `FAILED: ${sttResponse.error}`,
		});
		return {
			phrase,
			stages,
			totalMs: performance.now() - totalStart,
			transcription: `[ERROR: ${sttResponse.error}]`,
			match: false,
			similarity: 0,
		};
	}

	const transcription = sttResponse.result?.text || '';
	const confidence = sttResponse.result?.confidence || 0;
	stages.push({
		stage: 'STT (Whisper)',
		durationMs: sttMs,
		detail: `"${transcription}" (confidence: ${confidence.toFixed(2)})`,
	});

	// Stage 4: Comparison
	const similarity = wordSimilarity(phrase, transcription);
	const match = similarity >= 0.5; // At least half the words match

	const totalMs = performance.now() - totalStart;

	return { phrase, stages, totalMs, transcription, match, similarity };
}

async function main() {
	console.log('=== Layer 5: TTS → STT Round-Trip Test ===\n');

	if (!fs.existsSync(SOCKET_PATH)) {
		console.error(`❌ Socket not found: ${SOCKET_PATH}`);
		console.error('   Start the system first: npm start');
		process.exit(1);
	}

	const client = new RustCoreIPCClient(SOCKET_PATH);
	await client.connect();
	console.log('✅ Connected to continuum-core\n');

	// Warmup call (first TTS call is slow due to ONNX initialization)
	console.log('⏳ Warming up Kokoro TTS...');
	const warmupStart = performance.now();
	await client.voiceSynthesize('warmup', 'af', 'kokoro');
	console.log(`   Warmup took ${(performance.now() - warmupStart).toFixed(0)}ms\n`);

	const phrases = [
		'Hello world',
		'The quick brown fox jumps over the lazy dog',
		'Testing one two three four five',
	];

	const results: RoundTripResult[] = [];

	for (const phrase of phrases) {
		console.log(`📝 Input: "${phrase}"`);

		try {
			const result = await testRoundTrip(client, phrase);
			results.push(result);

			// Print stages
			for (const stage of result.stages) {
				const bar = '█'.repeat(Math.max(1, Math.round(stage.durationMs / 100)));
				console.log(`   ${stage.stage.padEnd(20)} ${stage.durationMs.toFixed(1).padStart(8)}ms ${bar} ${stage.detail}`);
			}

			const icon = result.match ? '✅' : '❌';
			console.log(`   ${icon} Transcription: "${result.transcription}" (similarity: ${(result.similarity * 100).toFixed(0)}%)`);
			console.log(`   Total: ${result.totalMs.toFixed(0)}ms\n`);
		} catch (e) {
			console.log(`   ❌ Error: ${(e as Error).message}\n`);
			results.push({
				phrase,
				stages: [],
				totalMs: 0,
				transcription: `[ERROR: ${(e as Error).message}]`,
				match: false,
				similarity: 0,
			});
		}
	}

	client.disconnect();

	// Summary
	console.log('=== Summary ===');
	const passed = results.filter(r => r.match).length;
	const failed = results.filter(r => !r.match).length;
	console.log(`${passed} matched, ${failed} mismatched out of ${results.length} phrases\n`);

	// Timing summary
	if (results.length > 0 && results.some(r => r.stages.length > 0)) {
		console.log('Average per-stage timing:');
		const stageNames = ['TTS (Kokoro)', 'Base64 encode', 'STT (Whisper)'];
		for (const name of stageNames) {
			const times = results
				.flatMap(r => r.stages)
				.filter(s => s.stage === name)
				.map(s => s.durationMs);
			if (times.length > 0) {
				const avg = times.reduce((a, b) => a + b, 0) / times.length;
				const min = Math.min(...times);
				const max = Math.max(...times);
				console.log(`  ${name.padEnd(20)} avg: ${avg.toFixed(0)}ms, min: ${min.toFixed(0)}ms, max: ${max.toFixed(0)}ms`);
			}
		}
		const totalAvg = results.map(r => r.totalMs).reduce((a, b) => a + b, 0) / results.length;
		console.log(`  ${'TOTAL'.padEnd(20)} avg: ${totalAvg.toFixed(0)}ms`);
	}

	if (failed > 0) {
		console.log('\nFailed phrases:');
		for (const r of results.filter(r => !r.match)) {
			console.log(`  ❌ "${r.phrase}" → "${r.transcription}" (${(r.similarity * 100).toFixed(0)}%)`);
		}
		process.exit(1);
	}
}

main().catch(e => {
	console.error('Fatal error:', e);
	process.exit(1);
});
