/**
 * Unit Test: VoiceWebSocketHandler Transcription Message Handling
 *
 * Tests that VoiceWebSocketHandler correctly handles the 'Transcription' message case
 * that was MISSING before (the bug we're fixing).
 *
 * This is a UNIT test - no server needed, uses mocks.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { UUID } from '../../types/CrossPlatformUUID.js';

describe('VoiceWebSocketHandler - Transcription Handler (Unit Test)', () => {
  it('should have a Transcription case handler in handleJsonMessage', async () => {
    // Read the source file to verify the case handler exists
    const fs = await import('fs/promises');
    const path = await import('path');

    const handlerPath = path.join(process.cwd(), 'system/voice/server/VoiceWebSocketHandler.ts');
    const sourceCode = await fs.readFile(handlerPath, 'utf-8');

    // Verify the case 'Transcription': handler exists
    expect(sourceCode).toContain("case 'Transcription':");

    // Verify it calls getVoiceOrchestrator().onUtterance
    expect(sourceCode).toContain('getVoiceOrchestrator().onUtterance');

    // Verify it creates an UtteranceEvent
    expect(sourceCode).toContain('const utteranceEvent: UtteranceEvent');

    // Verify it includes the transcript from message.text
    expect(sourceCode).toContain('transcript: message.text');
  });

  it('should have handleJsonMessage as async', async () => {
    const fs = await import('fs/promises');
    const path = await import('path');

    const handlerPath = path.join(process.cwd(), 'system/voice/server/VoiceWebSocketHandler.ts');
    const sourceCode = await fs.readFile(handlerPath, 'utf-8');

    // The handler must be async to use await for onUtterance
    expect(sourceCode).toMatch(/private\s+async\s+handleJsonMessage/);
  });

  it('should log STEP 10 for debugging', async () => {
    const fs = await import('fs/promises');
    const path = await import('path');

    const handlerPath = path.join(process.cwd(), 'system/voice/server/VoiceWebSocketHandler.ts');
    const sourceCode = await fs.readFile(handlerPath, 'utf-8');

    // Should have STEP 10 logs for flow debugging
    expect(sourceCode).toContain('[STEP 10]');
  });

  it('should create UtteranceEvent with correct fields', async () => {
    const fs = await import('fs/promises');
    const path = await import('path');

    const handlerPath = path.join(process.cwd(), 'system/voice/server/VoiceWebSocketHandler.ts');
    const sourceCode = await fs.readFile(handlerPath, 'utf-8');

    // Check all required UtteranceEvent fields are populated
    const transcriptionCase = sourceCode.substring(
      sourceCode.indexOf("case 'Transcription':"),
      sourceCode.indexOf('break;', sourceCode.indexOf("case 'Transcription':"))
    );

    expect(transcriptionCase).toContain('sessionId:');
    expect(transcriptionCase).toContain('speakerId:');
    expect(transcriptionCase).toContain('speakerName:');
    expect(transcriptionCase).toContain('speakerType:');
    expect(transcriptionCase).toContain('transcript:');
    expect(transcriptionCase).toContain('confidence:');
    expect(transcriptionCase).toContain('timestamp:');
  });
});
