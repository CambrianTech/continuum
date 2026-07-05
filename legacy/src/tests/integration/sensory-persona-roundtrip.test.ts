/**
 * Sensory Persona Roundtrip — Position 2 alpha contract test
 *
 * Codifies the live sensory loop a STANDARD PERSONA must satisfy per #1072:
 * resolve a multimodal model (Chat + Vision + AudioInput + AudioOutput) →
 * spawn LiveKitAgent into a real WebRTC room → publish a question as TTS
 * audio + a known test image as a video frame → wait for the persona's
 * response audio AND transcription → assert transcription mentions the
 * image content (proves vision was wired) AND audio was published (proves
 * TTS reached the room).
 *
 * Failing-loud test today; passes as Position 1 (resolver with
 * RequirementProfile::StandardPersona) and Position 3 (Qwen multimodal GPU
 * kernels in llama.cpp/Candle) land. The bar is the test, not the impl.
 *
 * Loud-fail buckets — every failure path categorized so an operator can
 * grep the result instead of digging through logs:
 *
 *   no_qualified_model      — resolver returned no Standard-Persona-capable model
 *   persona_failed_to_join  — LiveKitAgent spawn errored or never joined
 *   no_audio_published      — persona was in room but no TTS track ever appeared
 *   no_transcription        — STT listener never produced a transcription segment
 *   vision_blind            — transcription text doesn't mention any image content
 *   budget_exceeded         — first response > FIRST_RESPONSE_BUDGET_MS or
 *                             full response > ALL_RESPONSE_BUDGET_MS
 *
 * Per #1070 / #1072 standing rules: NO silent CPU fallback, NO degraded-mode
 * fallback (text-only is not a passing result), NO retry-on-failure (single
 * attempt, fail loud, surface the bucket).
 *
 * Run with:
 *   npx tsx src/tests/integration/sensory-persona-roundtrip.test.ts
 *
 * Prerequisites (today's failing run will report which are missing):
 *   - LiveKit server running on $LIVEKIT_URL
 *   - continuum-core IPC socket available
 *   - Position 1 resolver shipped (RequirementProfile::StandardPersona)
 *   - Position 3 Qwen multimodal kernels available on this host
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../core/continuum-core/bindings/RustCoreIPC';

// =============================================================================
// Failure bucket taxonomy — typed so operator can grep
// =============================================================================

export type SmokeFailureBucket =
  | 'no_qualified_model'
  | 'persona_failed_to_join'
  | 'no_audio_published'
  | 'no_transcription'
  | 'vision_blind'
  | 'budget_exceeded';

export interface SmokeFailure {
  bucket: SmokeFailureBucket;
  reason: string;
  dependencies?: string[];
}

export interface SmokeSuccess {
  persona_id: string;
  model_id: string;
  first_response_ms: number;
  full_response_ms: number;
  transcription: string;
  vision_terms_matched: string[];
}

export type SmokeResult =
  | { ok: true; success: SmokeSuccess }
  | { ok: false; failure: SmokeFailure };

// =============================================================================
// Budgets — per #1062 RecipeTurnBatchPlan first/all-response budgets
// =============================================================================

const FIRST_RESPONSE_BUDGET_MS = 30_000;   // first audio frame from persona
const ALL_RESPONSE_BUDGET_MS = 60_000;     // full audio response + transcription
const TEST_ROOM_PREFIX = 'sensory-smoke';

// =============================================================================
// Test image — a known set of visual elements the persona should describe
// =============================================================================

interface TestImage {
  /** PNG/JPEG bytes the persona will see as a video frame */
  bytes: Buffer;
  /** Words a competent vision model should produce when asked 'what's in the image?' */
  expected_terms: string[];
}

function generateTestImageWithKnownContent(): TestImage {
  // Reuse the colored-quadrants test pattern from sensory_pipeline_test.rs
  // (Red top-left, Green top-right, Blue bottom-left, White bottom-right).
  // A multimodal model that sees this image should mention at least one of
  // ['red', 'green', 'blue', 'white', 'quadrant', 'square', 'color'] in its
  // response. If transcription mentions ZERO of these, vision is blind —
  // the persona either didn't receive the image or processed it as text-only.
  const width = 256;
  const height = 256;
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      let r = 0, g = 0, b = 0;
      if (x < width / 2 && y < height / 2) r = 255;
      else if (x >= width / 2 && y < height / 2) g = 255;
      else if (x < width / 2 && y >= height / 2) b = 255;
      else { r = 255; g = 255; b = 255; }
      rgba[i] = r;
      rgba[i + 1] = g;
      rgba[i + 2] = b;
      rgba[i + 3] = 255;
    }
  }
  return {
    bytes: rgba,
    expected_terms: ['red', 'green', 'blue', 'white', 'quadrant', 'square', 'color', 'corner'],
  };
}

// =============================================================================
// Smoke runner
// =============================================================================

export async function runSensoryPersonaSmoke(): Promise<SmokeResult> {
  const ipc = new RustCoreIPCClient(getContinuumCoreSocketPath());
  await ipc.connect();

  // STEP 1 — resolve a Standard-Persona-capable model.
  //
  // Calls Position 1's cognition/resolve-model IPC with
  // RequirementProfile::StandardPersona. The resolver is the one that
  // enforces 'Chat + Vision + AudioInput + AudioOutput on GPU/UMA, no
  // silent CPU fallback'. Until Position 1 ships, this returns
  // no_qualified_model with the reason describing the missing API.
  let resolved: { model_id: string; provider_id: string; target_silicon: string } | undefined;
  try {
    const response = await ipc.request({
      command: 'cognition/resolve-model',
      request: {
        profile: 'standard_persona',
        host: detectHostCapability(),
      },
    });
    if (!response.success || !response.result) {
      return failBucket('no_qualified_model', response.error ?? 'resolver returned no model', [
        'depends on Position 1: cognition/resolve-model IPC + RequirementProfile::StandardPersona',
        'depends on Position 3: a Qwen multimodal GGUF actually loadable on this host',
      ]);
    }
    resolved = response.result;
  } catch (e) {
    return failBucket(
      'no_qualified_model',
      `cognition/resolve-model IPC unavailable: ${e instanceof Error ? e.message : String(e)}`,
      ['Position 1 not merged — IPC handler not registered'],
    );
  }

  // STEP 2 — spawn LiveKitAgent for resolved persona + join test room.
  const roomName = `${TEST_ROOM_PREFIX}-${Date.now()}`;
  let agentJoinedAt: number | undefined;
  try {
    const joinResponse = await ipc.request({
      command: 'live/spawn-persona-agent',
      request: {
        room: roomName,
        persona_id: `smoke-${Date.now()}`,
        model_id: resolved!.model_id,
        provider_id: resolved!.provider_id,
      },
    });
    if (!joinResponse.success) {
      return failBucket(
        'persona_failed_to_join',
        joinResponse.error ?? 'spawn returned non-success',
        ['continuum-core LiveKitAgent must accept resolved-model handle'],
      );
    }
    agentJoinedAt = Date.now();
  } catch (e) {
    return failBucket(
      'persona_failed_to_join',
      `live/spawn-persona-agent IPC error: ${e instanceof Error ? e.message : String(e)}`,
    );
  }

  // STEP 3 — publish a TTS question + a test image as a video frame.
  const image = generateTestImageWithKnownContent();
  const question = "What's in the image?";
  await ipc.request({
    command: 'live/publish-test-stimulus',
    request: {
      room: roomName,
      audio_text: question,
      video_rgba: image.bytes.toString('base64'),
      width: 256,
      height: 256,
    },
  });

  // STEP 4 — poll for persona response: audio frames + transcription.
  const startWait = Date.now();
  let firstAudioMs: number | undefined;
  let transcription: string | undefined;
  while (Date.now() - startWait < ALL_RESPONSE_BUDGET_MS) {
    const status = await ipc.request({
      command: 'live/get-room-state',
      request: { room: roomName },
    });
    const state = status.result as {
      persona_audio_published: boolean;
      transcription_segments: Array<{ text: string; participant: string }>;
    } | undefined;
    if (!state) break;
    if (state.persona_audio_published && firstAudioMs === undefined) {
      firstAudioMs = Date.now() - startWait;
      if (firstAudioMs > FIRST_RESPONSE_BUDGET_MS) {
        return failBucket(
          'budget_exceeded',
          `first audio at ${firstAudioMs}ms exceeded budget ${FIRST_RESPONSE_BUDGET_MS}ms`,
        );
      }
    }
    const personaSegments = state.transcription_segments.filter((s) => s.participant !== 'human');
    if (personaSegments.length > 0) {
      transcription = personaSegments.map((s) => s.text).join(' ');
      break;
    }
    await sleep(500);
  }

  if (firstAudioMs === undefined) {
    return failBucket(
      'no_audio_published',
      `no persona TTS track appeared within ${ALL_RESPONSE_BUDGET_MS}ms`,
    );
  }
  if (!transcription) {
    return failBucket(
      'no_transcription',
      `persona audio published but no STT transcription within ${ALL_RESPONSE_BUDGET_MS}ms`,
    );
  }

  // STEP 5 — assert transcription mentions image content (proves vision worked).
  const lower = transcription.toLowerCase();
  const matched = image.expected_terms.filter((term) => lower.includes(term));
  if (matched.length === 0) {
    return failBucket(
      'vision_blind',
      `persona responded but transcription "${transcription}" mentioned none of ${image.expected_terms.join(', ')} — vision was not wired or model is text-only`,
    );
  }

  return {
    ok: true,
    success: {
      persona_id: `smoke-${Date.now()}`,
      model_id: resolved!.model_id,
      first_response_ms: firstAudioMs,
      full_response_ms: Date.now() - startWait,
      transcription,
      vision_terms_matched: matched,
    },
  };
}

// =============================================================================
// Helpers
// =============================================================================

function detectHostCapability(): { hw_capability_tier: string; available_memory_mb: number; primary_target_silicon: string } {
  // Stub today — Position 1 (or a separate boot-time hardware probe module)
  // owns the real implementation. Smoke test passes whatever it has and
  // lets the resolver fail-loud if it can't decide.
  return {
    hw_capability_tier: process.env.CONTINUUM_HW_CAPABILITY_TIER ?? 'M3UmaProMax',
    available_memory_mb: parseInt(process.env.CONTINUUM_AVAILABLE_MEMORY_MB ?? '16384', 10),
    primary_target_silicon: process.env.CONTINUUM_PRIMARY_SILICON ?? 'UnifiedMemory',
  };
}

function failBucket(
  bucket: SmokeFailureBucket,
  reason: string,
  dependencies?: string[],
): SmokeResult {
  return { ok: false, failure: { bucket, reason, dependencies } };
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

// =============================================================================
// Entry point
// =============================================================================

async function main(): Promise<void> {
  console.log('🎙️  sensory-persona-roundtrip smoke starting…');
  const result = await runSensoryPersonaSmoke();
  if (result.ok) {
    console.log('✅ PASS', JSON.stringify(result.success, null, 2));
    process.exit(0);
  }
  console.error('❌ FAIL bucket=' + result.failure.bucket);
  console.error('   reason: ' + result.failure.reason);
  if (result.failure.dependencies?.length) {
    console.error('   blockers:');
    for (const d of result.failure.dependencies) console.error('     - ' + d);
  }
  process.exit(1);
}

if (require.main === module) {
  main().catch((e) => {
    console.error('❌ FAIL bucket=persona_failed_to_join (unhandled exception)');
    console.error(e);
    process.exit(1);
  });
}
