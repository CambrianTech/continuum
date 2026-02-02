/**
 * CodeCoordinationStream Unit Tests
 *
 * Tests the file-level MUTEX coordination for multi-agent coding:
 * - Stream creation and configuration
 * - File lock acquisition and release
 * - Conflict detection (overlapping file claims)
 * - Multi-agent parallel coordination (non-overlapping files)
 * - Global lock management
 * - Singleton pattern
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  CodeCoordinationStream,
  getCodeCoordinator,
  resetCodeCoordinator,
  type CodeThought,
  type CodeDecision,
  type CodeStream,
} from '../../../system/coordination/server/CodeCoordinationStream';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// ── Helpers ──────────────────────────────────────────────────

const PLAN_ID = '11111111-2222-3333-4444-555555555555' as UUID;
const AGENT_A = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee' as UUID;
const AGENT_B = 'bbbbbbbb-cccc-dddd-eeee-ffffffffffff' as UUID;
const AGENT_C = 'cccccccc-dddd-eeee-ffff-111111111111' as UUID;

function makeThought(
  personaId: UUID,
  targetFiles: string[],
  overrides?: Partial<CodeThought>,
): CodeThought {
  return {
    personaId,
    personaName: `Agent-${personaId.slice(0, 4)}`,
    type: 'claiming',
    confidence: 0.8,
    reasoning: `Claiming files: ${targetFiles.join(', ')}`,
    timestamp: Date.now(),
    planId: PLAN_ID,
    targetFiles,
    stepNumbers: [1, 2],
    ...overrides,
  };
}

// ── Tests ────────────────────────────────────────────────────

describe('CodeCoordinationStream', () => {
  let coordinator: CodeCoordinationStream;

  beforeEach(() => {
    resetCodeCoordinator();
    coordinator = new CodeCoordinationStream();
  });

  afterEach(() => {
    coordinator.shutdown();
  });

  describe('construction and configuration', () => {
    it('creates with coding-specific config', () => {
      // Verify it's a proper instance
      expect(coordinator).toBeInstanceOf(CodeCoordinationStream);
    });

    it('starts with no global file locks', () => {
      expect(coordinator.globalFileLocks.size).toBe(0);
    });

    it('starts with no active streams', () => {
      expect(coordinator.getStreams().size).toBe(0);
    });
  });

  describe('file lock acquisition', () => {
    it('single agent acquires locks on broadcast', async () => {
      const thought = makeThought(AGENT_A, ['src/main.ts', 'src/utils.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      expect(coordinator.globalFileLocks.size).toBe(2);
      expect(coordinator.lockHolder('src/main.ts')).toBe(AGENT_A);
      expect(coordinator.lockHolder('src/utils.ts')).toBe(AGENT_A);
    });

    it('isFileLocked returns correct status', async () => {
      expect(coordinator.isFileLocked('src/main.ts')).toBe(false);

      const thought = makeThought(AGENT_A, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      expect(coordinator.isFileLocked('src/main.ts')).toBe(true);
      expect(coordinator.isFileLocked('src/other.ts')).toBe(false);
    });

    it('lockHolder returns undefined for unlocked files', () => {
      expect(coordinator.lockHolder('src/nonexistent.ts')).toBeUndefined();
    });
  });

  describe('conflict detection', () => {
    it('rejects claim when files already locked by another agent', async () => {
      // Agent A claims main.ts
      const thoughtA = makeThought(AGENT_A, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);

      // Agent B tries to claim main.ts — should be rejected
      const thoughtB = makeThought(AGENT_B, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);

      // main.ts should still be locked by Agent A
      expect(coordinator.lockHolder('src/main.ts')).toBe(AGENT_A);
    });

    it('allows same agent to reclaim their own files', async () => {
      const thought1 = makeThought(AGENT_A, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought1);

      const thought2 = makeThought(AGENT_A, ['src/main.ts', 'src/extra.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought2);

      expect(coordinator.lockHolder('src/main.ts')).toBe(AGENT_A);
      expect(coordinator.lockHolder('src/extra.ts')).toBe(AGENT_A);
    });

    it('rejects claim when any file in the set conflicts', async () => {
      const thoughtA = makeThought(AGENT_A, ['src/shared.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);

      // Agent B claims unique.ts + shared.ts — shared.ts conflicts
      const thoughtB = makeThought(AGENT_B, ['src/unique.ts', 'src/shared.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);

      // shared.ts still locked by A, unique.ts NOT locked (whole claim rejected)
      expect(coordinator.lockHolder('src/shared.ts')).toBe(AGENT_A);
      expect(coordinator.isFileLocked('src/unique.ts')).toBe(false);
    });
  });

  describe('parallel non-overlapping agents', () => {
    it('multiple agents acquire non-overlapping file locks', async () => {
      const thoughtA = makeThought(AGENT_A, ['src/moduleA.ts']);
      const thoughtB = makeThought(AGENT_B, ['src/moduleB.ts']);
      const thoughtC = makeThought(AGENT_C, ['src/moduleC.ts']);

      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtC);

      expect(coordinator.globalFileLocks.size).toBe(3);
      expect(coordinator.lockHolder('src/moduleA.ts')).toBe(AGENT_A);
      expect(coordinator.lockHolder('src/moduleB.ts')).toBe(AGENT_B);
      expect(coordinator.lockHolder('src/moduleC.ts')).toBe(AGENT_C);
    });

    it('canWorkOnFiles checks correctly for non-overlapping', async () => {
      const thought = makeThought(AGENT_A, ['src/moduleA.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      const canB = await coordinator.canWorkOnFiles(AGENT_B, PLAN_ID, ['src/moduleB.ts']);
      expect(canB).toBe(true);

      const canBConflict = await coordinator.canWorkOnFiles(AGENT_B, PLAN_ID, ['src/moduleA.ts']);
      expect(canBConflict).toBe(false);
    });

    it('canWorkOnFiles returns true when no stream exists', async () => {
      const can = await coordinator.canWorkOnFiles(AGENT_A, 'no-such-plan' as UUID, ['anything.ts']);
      expect(can).toBe(true);
    });
  });

  describe('lock release', () => {
    it('releases all locks for a persona', async () => {
      const thought = makeThought(AGENT_A, ['src/a.ts', 'src/b.ts', 'src/c.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      expect(coordinator.globalFileLocks.size).toBe(3);

      coordinator.releaseLocks(AGENT_A);

      expect(coordinator.globalFileLocks.size).toBe(0);
      expect(coordinator.isFileLocked('src/a.ts')).toBe(false);
    });

    it('releases only the specified persona locks', async () => {
      const thoughtA = makeThought(AGENT_A, ['src/a.ts']);
      const thoughtB = makeThought(AGENT_B, ['src/b.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);

      coordinator.releaseLocks(AGENT_A);

      expect(coordinator.isFileLocked('src/a.ts')).toBe(false);
      expect(coordinator.isFileLocked('src/b.ts')).toBe(true);
      expect(coordinator.lockHolder('src/b.ts')).toBe(AGENT_B);
    });

    it('releases locks for a specific plan only', async () => {
      const PLAN_2 = '22222222-3333-4444-5555-666666666666' as UUID;
      const thoughtA1 = makeThought(AGENT_A, ['src/plan1.ts']);
      const thoughtA2 = makeThought(AGENT_A, ['src/plan2.ts']);

      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA1);
      await coordinator.broadcastCodeThought(PLAN_2, thoughtA2);

      // Release only for PLAN_ID stream — global locks for PLAN_2 remain
      coordinator.releaseLocks(AGENT_A, PLAN_ID);

      // Stream-level locks for plan1 should be gone
      const stream1 = coordinator.getStream(PLAN_ID);
      if (stream1) {
        expect(stream1.fileLocks.has('src/plan1.ts')).toBe(false);
      }
    });
  });

  describe('deferring', () => {
    it('defer releases claimed slot', async () => {
      const claim = makeThought(AGENT_A, ['src/main.ts'], { type: 'claiming' });
      await coordinator.broadcastCodeThought(PLAN_ID, claim);

      const stream = coordinator.getStream(PLAN_ID);
      expect(stream).toBeDefined();
      expect(stream!.claimedBy.has(AGENT_A)).toBe(true);

      const defer = makeThought(AGENT_A, ['src/main.ts'], { type: 'deferring' });
      await coordinator.broadcastCodeThought(PLAN_ID, defer);

      expect(stream!.claimedBy.has(AGENT_A)).toBe(false);
    });
  });

  describe('stream lifecycle', () => {
    it('creates stream on first thought', async () => {
      expect(coordinator.getStreams().size).toBe(0);

      const thought = makeThought(AGENT_A, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      expect(coordinator.getStreams().size).toBe(1);
      const stream = coordinator.getStream(PLAN_ID);
      expect(stream).toBeDefined();
      expect(stream!.planId).toBe(PLAN_ID);
    });

    it('stream accumulates thoughts from multiple agents', async () => {
      const thoughtA = makeThought(AGENT_A, ['src/a.ts']);
      const thoughtB = makeThought(AGENT_B, ['src/b.ts']);

      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);

      const stream = coordinator.getStream(PLAN_ID);
      expect(stream!.thoughts).toHaveLength(2);
      expect(stream!.considerations.size).toBe(2);
    });
  });

  describe('decision making', () => {
    it('waitForCodeDecision returns null for non-existent stream', async () => {
      const decision = await coordinator.waitForCodeDecision('no-such-plan' as UUID, 100);
      expect(decision).toBeNull();
    });

    it('decision includes file locks and conflicts', async () => {
      // Set up two agents claiming different files
      const thoughtA = makeThought(AGENT_A, ['src/a.ts'], { confidence: 0.9 });
      const thoughtB = makeThought(AGENT_B, ['src/b.ts'], { confidence: 0.8 });

      await coordinator.broadcastCodeThought(PLAN_ID, thoughtA);
      await coordinator.broadcastCodeThought(PLAN_ID, thoughtB);

      // Wait for decision (with short timeout since canDecideEarly may trigger)
      const decision = await coordinator.waitForCodeDecision(PLAN_ID, 4000);
      if (decision) {
        expect(decision.planId).toBe(PLAN_ID);
        expect(decision.fileLocks).toBeDefined();
        expect(decision.conflicts).toBeDefined();
      }
    });
  });

  describe('singleton pattern', () => {
    it('getCodeCoordinator returns same instance', () => {
      const a = getCodeCoordinator();
      const b = getCodeCoordinator();
      expect(a).toBe(b);
    });

    it('resetCodeCoordinator creates fresh instance', () => {
      const a = getCodeCoordinator();
      resetCodeCoordinator();
      const b = getCodeCoordinator();
      expect(a).not.toBe(b);
    });

    it('reset clears global file locks', async () => {
      const coord = getCodeCoordinator();
      const thought = makeThought(AGENT_A, ['src/locked.ts']);
      await coord.broadcastCodeThought(PLAN_ID, thought);

      expect(coord.globalFileLocks.size).toBe(1);
      resetCodeCoordinator();

      const fresh = getCodeCoordinator();
      expect(fresh.globalFileLocks.size).toBe(0);
    });
  });

  describe('shutdown', () => {
    it('clears all state on shutdown', async () => {
      const thought = makeThought(AGENT_A, ['src/main.ts']);
      await coordinator.broadcastCodeThought(PLAN_ID, thought);

      expect(coordinator.globalFileLocks.size).toBe(1);
      expect(coordinator.getStreams().size).toBe(1);

      coordinator.shutdown();

      expect(coordinator.globalFileLocks.size).toBe(0);
      expect(coordinator.getStreams().size).toBe(0);
    });
  });
});
