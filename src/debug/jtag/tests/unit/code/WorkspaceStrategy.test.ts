/**
 * WorkspaceStrategy Unit Tests
 *
 * Tests workspace creation routing (sandbox vs worktree),
 * handle tracking, deduplication, and cleanup.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { WorkspaceStrategy } from '../../../system/code/server/WorkspaceStrategy';
import type { WorkspaceConfig } from '../../../system/code/server/WorkspaceStrategy';

// Mock Commands.execute (used for worktree init/clean)
const mockExecute = vi.fn();
vi.mock('../../../system/core/shared/Commands', () => ({
  Commands: {
    execute: (...args: unknown[]) => mockExecute(...args),
  },
}));

// Mock CodeDaemon.createWorkspace
const mockCreateWorkspace = vi.fn().mockResolvedValue(undefined);
vi.mock('../../../daemons/code-daemon/shared/CodeDaemon', () => ({
  CodeDaemon: {
    createWorkspace: (...args: unknown[]) => mockCreateWorkspace(...args),
  },
}));

// Mock Logger
vi.mock('../../../system/core/logging/Logger', () => ({
  Logger: {
    create: () => ({
      debug: () => {},
      info: () => {},
      warn: () => {},
      error: () => {},
    }),
  },
}));

// Mock fs
vi.mock('fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  mkdirSync: vi.fn(),
}));

describe('WorkspaceStrategy', () => {
  beforeEach(() => {
    mockExecute.mockReset();
    mockCreateWorkspace.mockReset().mockResolvedValue(undefined);
    WorkspaceStrategy.resetTracking();
  });

  describe('sandbox mode', () => {
    it('creates sandbox workspace with correct handle and path', async () => {
      const config: WorkspaceConfig = {
        personaId: 'persona-123',
        mode: 'sandbox',
      };

      const result = await WorkspaceStrategy.create(config);

      expect(result.mode).toBe('sandbox');
      expect(result.handle).toBe('persona-123');
      expect(result.workspaceDir).toContain('.continuum/personas/persona-123/workspace');
      expect(result.branch).toBeUndefined();
    });

    it('registers with CodeDaemon including jtagRoot as read root', async () => {
      const config: WorkspaceConfig = {
        personaId: 'persona-456',
        mode: 'sandbox',
      };

      await WorkspaceStrategy.create(config);

      expect(mockCreateWorkspace).toHaveBeenCalledTimes(1);
      const [handle, workspaceDir, readRoots] = mockCreateWorkspace.mock.calls[0];
      expect(handle).toBe('persona-456');
      expect(workspaceDir).toContain('.continuum/personas/persona-456/workspace');
      expect(readRoots).toHaveLength(1);
      expect(readRoots[0]).toBe(process.cwd());
    });

    it('deduplicates — second call returns cached result without re-registering', async () => {
      const config: WorkspaceConfig = {
        personaId: 'persona-789',
        mode: 'sandbox',
      };

      const first = await WorkspaceStrategy.create(config);
      const second = await WorkspaceStrategy.create(config);

      expect(first.handle).toBe(second.handle);
      expect(first.workspaceDir).toBe(second.workspaceDir);
      // CodeDaemon.createWorkspace only called once
      expect(mockCreateWorkspace).toHaveBeenCalledTimes(1);
    });

    it('tracks initialized state via isInitialized', async () => {
      expect(WorkspaceStrategy.isInitialized('persona-abc')).toBe(false);

      await WorkspaceStrategy.create({
        personaId: 'persona-abc',
        mode: 'sandbox',
      });

      expect(WorkspaceStrategy.isInitialized('persona-abc')).toBe(true);
    });
  });

  describe('worktree mode', () => {
    it('creates worktree workspace via workspace/git/workspace/init', async () => {
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-branch',
        branch: 'ai/fix-bug',
      });

      const config: WorkspaceConfig = {
        personaId: 'persona-wt',
        mode: 'worktree',
        taskSlug: 'fix-bug',
        sparsePaths: ['src/system/code/', 'docs/'],
      };

      const result = await WorkspaceStrategy.create(config);

      expect(result.mode).toBe('worktree');
      expect(result.handle).toBe('worktree-persona-wt-fix-bug');
      expect(result.workspaceDir).toBe('/tmp/worktrees/ai-branch');
      expect(result.branch).toBe('ai/fix-bug');
    });

    it('calls workspace/git/workspace/init with correct params', async () => {
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-work',
        branch: 'ai/work',
      });

      await WorkspaceStrategy.create({
        personaId: 'persona-wt2',
        mode: 'worktree',
        taskSlug: 'work',
        sparsePaths: ['src/'],
      });

      expect(mockExecute).toHaveBeenCalledWith(
        'workspace/git/workspace/init',
        {
          personaId: 'persona-wt2',
          branch: 'ai/work',
          paths: ['src/'],
        }
      );
    });

    it('registers with CodeDaemon with empty read roots (worktree IS the repo)', async () => {
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-test',
        branch: 'ai/test',
      });

      await WorkspaceStrategy.create({
        personaId: 'persona-wt3',
        mode: 'worktree',
        taskSlug: 'test',
        sparsePaths: ['src/'],
      });

      expect(mockCreateWorkspace).toHaveBeenCalledWith(
        'worktree-persona-wt3-test',
        '/tmp/worktrees/ai-test',
        []
      );
    });

    it('throws when sparsePaths is empty', async () => {
      await expect(
        WorkspaceStrategy.create({
          personaId: 'persona-fail',
          mode: 'worktree',
          sparsePaths: [],
        })
      ).rejects.toThrow('worktree mode requires sparsePaths');
    });

    it('throws when sparsePaths is undefined', async () => {
      await expect(
        WorkspaceStrategy.create({
          personaId: 'persona-fail2',
          mode: 'worktree',
        })
      ).rejects.toThrow('worktree mode requires sparsePaths');
    });

    it('throws when workspace/git/workspace/init fails', async () => {
      mockExecute.mockResolvedValue({
        success: false,
        error: { message: 'Git worktree creation failed: branch already exists' },
      });

      await expect(
        WorkspaceStrategy.create({
          personaId: 'persona-fail3',
          mode: 'worktree',
          sparsePaths: ['src/'],
        })
      ).rejects.toThrow('worktree creation failed');
    });

    it('defaults taskSlug to work when not provided', async () => {
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-work',
        branch: 'ai/work',
      });

      const result = await WorkspaceStrategy.create({
        personaId: 'persona-default',
        mode: 'worktree',
        sparsePaths: ['src/'],
      });

      expect(result.handle).toBe('worktree-persona-default-work');
      expect(mockExecute).toHaveBeenCalledWith(
        'workspace/git/workspace/init',
        expect.objectContaining({ branch: 'ai/work' })
      );
    });

    it('deduplicates worktree workspaces', async () => {
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-dedup',
        branch: 'ai/dedup',
      });

      const config: WorkspaceConfig = {
        personaId: 'persona-dedup',
        mode: 'worktree',
        taskSlug: 'dedup',
        sparsePaths: ['src/'],
      };

      await WorkspaceStrategy.create(config);
      const second = await WorkspaceStrategy.create(config);

      // Only one init call
      expect(mockExecute).toHaveBeenCalledTimes(1);
      expect(second.mode).toBe('worktree');
    });
  });

  describe('cleanup', () => {
    it('calls workspace/git/workspace/clean for worktree handles', async () => {
      mockExecute.mockResolvedValue({ success: true });

      await WorkspaceStrategy.cleanup('worktree-persona-abc-task');

      expect(mockExecute).toHaveBeenCalledWith(
        'workspace/git/workspace/clean',
        { force: false, deleteBranch: false }
      );
    });

    it('passes force and deleteBranch options', async () => {
      mockExecute.mockResolvedValue({ success: true });

      await WorkspaceStrategy.cleanup('worktree-persona-abc-task', {
        force: true,
        deleteBranch: true,
      });

      expect(mockExecute).toHaveBeenCalledWith(
        'workspace/git/workspace/clean',
        { force: true, deleteBranch: true }
      );
    });

    it('skips cleanup for non-worktree handles', async () => {
      await WorkspaceStrategy.cleanup('persona-123');

      expect(mockExecute).not.toHaveBeenCalled();
    });

    it('removes handle from tracking after cleanup', async () => {
      // First create a worktree workspace
      mockExecute.mockResolvedValue({
        success: true,
        workspacePath: '/tmp/worktrees/ai-cleanup',
        branch: 'ai/cleanup',
      });

      await WorkspaceStrategy.create({
        personaId: 'persona-cleanup',
        mode: 'worktree',
        taskSlug: 'cleanup',
        sparsePaths: ['src/'],
      });

      expect(WorkspaceStrategy.isInitialized('worktree-persona-cleanup-cleanup')).toBe(true);

      // Now clean up
      mockExecute.mockResolvedValue({ success: true });
      await WorkspaceStrategy.cleanup('worktree-persona-cleanup-cleanup');

      expect(WorkspaceStrategy.isInitialized('worktree-persona-cleanup-cleanup')).toBe(false);
    });

    it('handles cleanup errors gracefully without throwing', async () => {
      mockExecute.mockRejectedValue(new Error('Git error'));

      // Should not throw
      await WorkspaceStrategy.cleanup('worktree-persona-err-task');
    });
  });

  describe('resetTracking', () => {
    it('clears all tracked workspaces', async () => {
      await WorkspaceStrategy.create({
        personaId: 'persona-reset',
        mode: 'sandbox',
      });

      expect(WorkspaceStrategy.isInitialized('persona-reset')).toBe(true);

      WorkspaceStrategy.resetTracking();

      expect(WorkspaceStrategy.isInitialized('persona-reset')).toBe(false);
    });
  });
});
