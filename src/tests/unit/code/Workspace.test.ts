/**
 * Workspace Unit Tests
 *
 * Tests that the Workspace class:
 * - Creates via WorkspaceStrategy and returns a bound handle
 * - Delegates all operations to CodeDaemon with the retained handle
 * - Provides fromExisting() for resuming previously created workspaces
 * - Cleans up via WorkspaceStrategy.cleanup()
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Workspace } from '../../../system/code/server/Workspace';
import { WorkspaceStrategy } from '../../../system/code/server/WorkspaceStrategy';
import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import { CodeVerify } from '../../../commands/code/verify/shared/CodeVerifyTypes';

// ── Mock dependencies ──────────────────────────────────────

vi.mock('../../../system/code/server/WorkspaceStrategy', () => ({
  WorkspaceStrategy: {
    create: vi.fn(),
    cleanup: vi.fn(),
  },
}));

vi.mock('../../../daemons/code-daemon/shared/CodeDaemon', () => ({
  CodeDaemon: {
    workspaceRead: vi.fn(),
    workspaceWrite: vi.fn(),
    workspaceEdit: vi.fn(),
    workspaceDelete: vi.fn(),
    workspaceDiff: vi.fn(),
    workspaceSearch: vi.fn(),
    workspaceTree: vi.fn(),
    workspaceUndo: vi.fn(),
    workspaceHistory: vi.fn(),
    workspaceGitStatus: vi.fn(),
    workspaceGitDiff: vi.fn(),
    workspaceGitLog: vi.fn(),
    workspaceGitAdd: vi.fn(),
    workspaceGitCommit: vi.fn(),
    workspaceGitPush: vi.fn(),
    // Shell session methods
    shellCreate: vi.fn(),
    shellExecute: vi.fn(),
    shellPoll: vi.fn(),
    shellKill: vi.fn(),
    shellCd: vi.fn(),
    shellStatus: vi.fn(),
    shellDestroy: vi.fn(),
    // Shell watch + sentinel
    shellWatch: vi.fn(),
    shellSentinel: vi.fn(),
  },
}));

vi.mock('../../../commands/code/verify/shared/CodeVerifyTypes', () => ({
  CodeVerify: {
    execute: vi.fn(),
  },
}));

// ── Helpers ────────────────────────────────────────────────

const PERSONA_ID = 'test-persona-abc';
const WORKSPACE_DIR = '/tmp/workspace/test';
const HANDLE = `worktree-${PERSONA_ID}-fix-auth`;
const BRANCH = 'ai/fix-auth';

function mockWorkspaceCreate() {
  vi.mocked(WorkspaceStrategy.create).mockResolvedValue({
    handle: HANDLE,
    workspaceDir: WORKSPACE_DIR,
    mode: 'worktree',
    branch: BRANCH,
  });
}

// ── Tests ──────────────────────────────────────────────────

describe('Workspace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('creation', () => {
    it('creates via WorkspaceStrategy and exposes handle, dir, mode, branch', async () => {
      mockWorkspaceCreate();

      const ws = await Workspace.create({
        personaId: PERSONA_ID,
        mode: 'worktree',
        taskSlug: 'fix-auth',
        sparsePaths: ['src/'],
      });

      expect(WorkspaceStrategy.create).toHaveBeenCalledWith({
        personaId: PERSONA_ID,
        mode: 'worktree',
        taskSlug: 'fix-auth',
        sparsePaths: ['src/'],
      });

      expect(ws.handle).toBe(HANDLE);
      expect(ws.dir).toBe(WORKSPACE_DIR);
      expect(ws.mode).toBe('worktree');
      expect(ws.branch).toBe(BRANCH);
    });

    it('creates sandbox workspace without branch', async () => {
      vi.mocked(WorkspaceStrategy.create).mockResolvedValue({
        handle: PERSONA_ID,
        workspaceDir: '/tmp/sandbox',
        mode: 'sandbox',
      });

      const ws = await Workspace.create({ personaId: PERSONA_ID, mode: 'sandbox' });

      expect(ws.handle).toBe(PERSONA_ID);
      expect(ws.mode).toBe('sandbox');
      expect(ws.branch).toBeUndefined();
    });

    it('fromExisting creates without calling WorkspaceStrategy', () => {
      const ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);

      expect(ws.handle).toBe(HANDLE);
      expect(ws.dir).toBe(WORKSPACE_DIR);
      expect(ws.mode).toBe('worktree');
      expect(ws.branch).toBe(BRANCH);
      expect(WorkspaceStrategy.create).not.toHaveBeenCalled();
    });
  });

  describe('file operations', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
    });

    it('read delegates to CodeDaemon.workspaceRead with handle', async () => {
      const mockResult = { content: 'file contents', lineCount: 10, filePath: 'src/auth.ts' };
      vi.mocked(CodeDaemon.workspaceRead).mockResolvedValue(mockResult as any);

      const result = await ws.read('src/auth.ts', 1, 10);

      expect(CodeDaemon.workspaceRead).toHaveBeenCalledWith(HANDLE, 'src/auth.ts', 1, 10);
      expect(result).toBe(mockResult);
    });

    it('write delegates to CodeDaemon.workspaceWrite with handle', async () => {
      const mockResult = { changeId: 'ch-1', filePath: 'new.ts' };
      vi.mocked(CodeDaemon.workspaceWrite).mockResolvedValue(mockResult as any);

      const result = await ws.write('new.ts', 'content', 'Created new file');

      expect(CodeDaemon.workspaceWrite).toHaveBeenCalledWith(HANDLE, 'new.ts', 'content', 'Created new file');
      expect(result).toBe(mockResult);
    });

    it('edit delegates to CodeDaemon.workspaceEdit with handle', async () => {
      const editMode = { editType: 'search_replace' as const, search: 'old', replace: 'new' };
      vi.mocked(CodeDaemon.workspaceEdit).mockResolvedValue({ changeId: 'ch-2' } as any);

      await ws.edit('src/auth.ts', editMode as any, 'Fix token check');

      expect(CodeDaemon.workspaceEdit).toHaveBeenCalledWith(HANDLE, 'src/auth.ts', editMode, 'Fix token check');
    });

    it('delete delegates to CodeDaemon.workspaceDelete with handle', async () => {
      vi.mocked(CodeDaemon.workspaceDelete).mockResolvedValue({ changeId: 'ch-3' } as any);

      await ws.delete('old-file.ts', 'Removed unused file');

      expect(CodeDaemon.workspaceDelete).toHaveBeenCalledWith(HANDLE, 'old-file.ts', 'Removed unused file');
    });

    it('diff delegates to CodeDaemon.workspaceDiff with handle', async () => {
      const editMode = { editType: 'search_replace' as const, search: 'a', replace: 'b' };
      vi.mocked(CodeDaemon.workspaceDiff).mockResolvedValue({ success: true, unified: '--- a\n+++ b' });

      const result = await ws.diff('file.ts', editMode as any);

      expect(CodeDaemon.workspaceDiff).toHaveBeenCalledWith(HANDLE, 'file.ts', editMode);
      expect(result.unified).toContain('---');
    });
  });

  describe('search and discovery', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
    });

    it('search delegates to CodeDaemon.workspaceSearch with handle', async () => {
      vi.mocked(CodeDaemon.workspaceSearch).mockResolvedValue({ matches: [], totalMatches: 0 } as any);

      await ws.search('TODO', '*.ts', 50);

      expect(CodeDaemon.workspaceSearch).toHaveBeenCalledWith(HANDLE, 'TODO', '*.ts', 50);
    });

    it('tree delegates to CodeDaemon.workspaceTree with handle', async () => {
      vi.mocked(CodeDaemon.workspaceTree).mockResolvedValue({ root: { name: '.' } } as any);

      await ws.tree('src/', 3, false);

      expect(CodeDaemon.workspaceTree).toHaveBeenCalledWith(HANDLE, 'src/', 3, false);
    });
  });

  describe('change tracking', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
    });

    it('undo delegates to CodeDaemon.workspaceUndo with handle', async () => {
      vi.mocked(CodeDaemon.workspaceUndo).mockResolvedValue({ undone: 1 } as any);

      await ws.undo('ch-1');

      expect(CodeDaemon.workspaceUndo).toHaveBeenCalledWith(HANDLE, 'ch-1', undefined);
    });

    it('history delegates to CodeDaemon.workspaceHistory with handle', async () => {
      vi.mocked(CodeDaemon.workspaceHistory).mockResolvedValue({ changes: [] } as any);

      await ws.history('src/auth.ts', 5);

      expect(CodeDaemon.workspaceHistory).toHaveBeenCalledWith(HANDLE, 'src/auth.ts', 5);
    });
  });

  describe('verification', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
    });

    it('verify delegates to CodeVerify.execute with handle as userId', async () => {
      vi.mocked(CodeVerify.execute).mockResolvedValue({ success: true } as any);

      await ws.verify(true, ['tests/auth.test.ts']);

      expect(CodeVerify.execute).toHaveBeenCalledWith({
        userId: HANDLE,
        typeCheck: true,
        testFiles: ['tests/auth.test.ts'],
      });
    });
  });

  describe('git operations', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
    });

    it('gitStatus delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitStatus).mockResolvedValue({ branch: BRANCH } as any);
      await ws.gitStatus();
      expect(CodeDaemon.workspaceGitStatus).toHaveBeenCalledWith(HANDLE);
    });

    it('gitDiff delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitDiff).mockResolvedValue({ success: true, diff: '' });
      await ws.gitDiff(true);
      expect(CodeDaemon.workspaceGitDiff).toHaveBeenCalledWith(HANDLE, true);
    });

    it('gitLog delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitLog).mockResolvedValue({ success: true, log: '' });
      await ws.gitLog(10);
      expect(CodeDaemon.workspaceGitLog).toHaveBeenCalledWith(HANDLE, 10);
    });

    it('gitAdd delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitAdd).mockResolvedValue({ staged: ['.'] });
      await ws.gitAdd(['.']);
      expect(CodeDaemon.workspaceGitAdd).toHaveBeenCalledWith(HANDLE, ['.']);
    });

    it('gitCommit delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitCommit).mockResolvedValue({ hash: 'abc123' });
      const result = await ws.gitCommit('Fix auth');
      expect(CodeDaemon.workspaceGitCommit).toHaveBeenCalledWith(HANDLE, 'Fix auth');
      expect(result.hash).toBe('abc123');
    });

    it('gitPush delegates with handle', async () => {
      vi.mocked(CodeDaemon.workspaceGitPush).mockResolvedValue({ output: 'pushed' });
      await ws.gitPush('origin', BRANCH);
      expect(CodeDaemon.workspaceGitPush).toHaveBeenCalledWith(HANDLE, 'origin', BRANCH);
    });
  });

  describe('shell session', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
      vi.mocked(CodeDaemon.shellCreate).mockResolvedValue({
        session_id: 'sess-1',
        persona_id: HANDLE,
        cwd: WORKSPACE_DIR,
        active_executions: 0,
        total_executions: 0,
      } as any);
    });

    it('exec auto-creates shell session on first call', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-1',
        status: 'completed',
        stdout: 'ok',
        stderr: null,
        exit_code: 0,
      } as any);

      await ws.exec('echo hello');

      expect(CodeDaemon.shellCreate).toHaveBeenCalledWith(HANDLE, WORKSPACE_DIR);
      expect(CodeDaemon.shellExecute).toHaveBeenCalledWith(HANDLE, 'echo hello', {
        timeoutMs: 30000,
        wait: true,
      });
    });

    it('exec only creates shell session once', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-1', status: 'completed',
      } as any);

      await ws.exec('echo 1');
      await ws.exec('echo 2');

      expect(CodeDaemon.shellCreate).toHaveBeenCalledTimes(1);
      expect(CodeDaemon.shellExecute).toHaveBeenCalledTimes(2);
    });

    it('exec passes custom timeout', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-1', status: 'completed',
      } as any);

      await ws.exec('cargo build', 120000);

      expect(CodeDaemon.shellExecute).toHaveBeenCalledWith(HANDLE, 'cargo build', {
        timeoutMs: 120000,
        wait: true,
      });
    });

    it('execAsync returns handle immediately (wait=false)', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-long',
        status: 'running',
        stdout: null,
        stderr: null,
        exit_code: null,
      } as any);

      const result = await ws.execAsync('npm run build');

      expect(CodeDaemon.shellExecute).toHaveBeenCalledWith(HANDLE, 'npm run build', {
        timeoutMs: undefined,
        wait: false,
      });
      expect(result.execution_id).toBe('exec-long');
      expect(result.status).toBe('running');
    });

    it('shellPoll delegates to CodeDaemon.shellPoll with handle', async () => {
      vi.mocked(CodeDaemon.shellPoll).mockResolvedValue({
        execution_id: 'exec-1',
        status: 'running',
        new_stdout: ['line 1', 'line 2'],
        new_stderr: [],
        exit_code: null,
        finished: false,
      } as any);

      const result = await ws.shellPoll('exec-1');

      expect(CodeDaemon.shellPoll).toHaveBeenCalledWith(HANDLE, 'exec-1');
      expect(result.new_stdout).toEqual(['line 1', 'line 2']);
      expect(result.finished).toBe(false);
    });

    it('shellKill delegates to CodeDaemon.shellKill with handle', async () => {
      vi.mocked(CodeDaemon.shellKill).mockResolvedValue();

      await ws.shellKill('exec-1');

      expect(CodeDaemon.shellKill).toHaveBeenCalledWith(HANDLE, 'exec-1');
    });

    it('shellCd auto-creates session and delegates', async () => {
      vi.mocked(CodeDaemon.shellCd).mockResolvedValue({ cwd: '/tmp/workspace/test/src' });

      const result = await ws.shellCd('src');

      expect(CodeDaemon.shellCreate).toHaveBeenCalledWith(HANDLE, WORKSPACE_DIR);
      expect(CodeDaemon.shellCd).toHaveBeenCalledWith(HANDLE, 'src');
      expect(result.cwd).toBe('/tmp/workspace/test/src');
    });

    it('shellStatus auto-creates session and delegates', async () => {
      vi.mocked(CodeDaemon.shellStatus).mockResolvedValue({
        session_id: 'sess-1',
        persona_id: HANDLE,
        cwd: WORKSPACE_DIR,
        active_executions: 0,
        total_executions: 3,
      } as any);

      const result = await ws.shellStatus();

      expect(CodeDaemon.shellCreate).toHaveBeenCalledWith(HANDLE, WORKSPACE_DIR);
      expect(CodeDaemon.shellStatus).toHaveBeenCalledWith(HANDLE);
      expect(result.total_executions).toBe(3);
    });
  });

  describe('shell watch + sentinel', () => {
    let ws: Workspace;

    beforeEach(() => {
      ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
      vi.mocked(CodeDaemon.shellCreate).mockResolvedValue({
        session_id: 'sess-1',
        persona_id: HANDLE,
        cwd: WORKSPACE_DIR,
        active_executions: 0,
        total_executions: 0,
      } as any);
    });

    it('sentinel delegates to CodeDaemon.shellSentinel with handle', async () => {
      vi.mocked(CodeDaemon.shellSentinel).mockResolvedValue({ applied: true, ruleCount: 2 });

      const rules = [
        { pattern: '^error', classification: 'Error' as const, action: 'Emit' as const },
        { pattern: '.*', classification: 'Verbose' as const, action: 'Suppress' as const },
      ];

      const result = await ws.sentinel('exec-1', rules);

      expect(CodeDaemon.shellSentinel).toHaveBeenCalledWith(HANDLE, 'exec-1', rules);
      expect(result.applied).toBe(true);
      expect(result.ruleCount).toBe(2);
    });

    it('watch auto-creates shell and delegates to CodeDaemon.shellWatch', async () => {
      const watchResponse = {
        execution_id: 'exec-1',
        lines: [
          { text: 'Compiling...', classification: 'Info', line_number: 0, stream: 'stdout', timestamp: Date.now() },
        ],
        finished: false,
        exit_code: undefined,
      };
      vi.mocked(CodeDaemon.shellWatch).mockResolvedValue(watchResponse as any);

      const result = await ws.watch('exec-1');

      expect(CodeDaemon.shellCreate).toHaveBeenCalledWith(HANDLE, WORKSPACE_DIR);
      expect(CodeDaemon.shellWatch).toHaveBeenCalledWith(HANDLE, 'exec-1');
      expect(result.lines).toHaveLength(1);
      expect(result.lines[0].text).toBe('Compiling...');
      expect(result.finished).toBe(false);
    });

    it('execWatch composes exec → sentinel → watch loop', async () => {
      // Mock execAsync
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-build',
        status: 'running',
        stdout: null,
        stderr: null,
        exit_code: null,
      } as any);

      // Mock sentinel
      vi.mocked(CodeDaemon.shellSentinel).mockResolvedValue({ applied: true, ruleCount: 1 });

      // Mock watch — first call returns output, second returns finished
      vi.mocked(CodeDaemon.shellWatch)
        .mockResolvedValueOnce({
          execution_id: 'exec-build',
          lines: [
            { text: 'Building...', classification: 'Info', line_number: 0, stream: 'stdout', timestamp: Date.now() },
          ],
          finished: false,
        } as any)
        .mockResolvedValueOnce({
          execution_id: 'exec-build',
          lines: [
            { text: 'Done', classification: 'Success', line_number: 1, stream: 'stdout', timestamp: Date.now() },
          ],
          finished: true,
          exit_code: 0,
        } as any);

      const rules = [
        { pattern: '.*', classification: 'Info' as const, action: 'Emit' as const },
      ];
      const collectedLines: any[] = [];

      const result = await ws.execWatch('cargo build', rules, (line) => {
        collectedLines.push(line);
      });

      // Verify composition: exec → sentinel → watch loop
      expect(CodeDaemon.shellExecute).toHaveBeenCalledWith(HANDLE, 'cargo build', {
        timeoutMs: undefined,
        wait: false,
      });
      expect(CodeDaemon.shellSentinel).toHaveBeenCalledWith(HANDLE, 'exec-build', rules);
      expect(CodeDaemon.shellWatch).toHaveBeenCalledTimes(2);

      // Verify all lines were collected
      expect(collectedLines).toHaveLength(2);
      expect(collectedLines[0].text).toBe('Building...');
      expect(collectedLines[1].text).toBe('Done');

      // Verify final response
      expect(result.finished).toBe(true);
      expect(result.exit_code).toBe(0);
    });

    it('execWatch works without sentinel rules', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-quick',
        status: 'running',
      } as any);

      vi.mocked(CodeDaemon.shellWatch).mockResolvedValueOnce({
        execution_id: 'exec-quick',
        lines: [],
        finished: true,
        exit_code: 0,
      } as any);

      const result = await ws.execWatch('echo hello');

      // No sentinel should be called
      expect(CodeDaemon.shellSentinel).not.toHaveBeenCalled();
      expect(result.finished).toBe(true);
    });

    it('execWatch works without onLine callback', async () => {
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({
        execution_id: 'exec-silent',
        status: 'running',
      } as any);

      vi.mocked(CodeDaemon.shellWatch).mockResolvedValueOnce({
        execution_id: 'exec-silent',
        lines: [
          { text: 'output', classification: 'Info', line_number: 0, stream: 'stdout', timestamp: Date.now() },
        ],
        finished: true,
        exit_code: 0,
      } as any);

      // Should not throw even without onLine callback
      const result = await ws.execWatch('echo hello');
      expect(result.finished).toBe(true);
    });
  });

  describe('lifecycle', () => {
    it('destroy delegates to WorkspaceStrategy.cleanup', async () => {
      vi.mocked(WorkspaceStrategy.cleanup).mockResolvedValue();

      const ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
      await ws.destroy({ force: true, deleteBranch: true });

      expect(WorkspaceStrategy.cleanup).toHaveBeenCalledWith(HANDLE, {
        force: true,
        deleteBranch: true,
      });
    });

    it('destroy cleans up shell session if one was created', async () => {
      vi.mocked(CodeDaemon.shellCreate).mockResolvedValue({} as any);
      vi.mocked(CodeDaemon.shellExecute).mockResolvedValue({ execution_id: 'e1' } as any);
      vi.mocked(CodeDaemon.shellDestroy).mockResolvedValue();
      vi.mocked(WorkspaceStrategy.cleanup).mockResolvedValue();

      const ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
      // Trigger shell creation
      await ws.exec('echo hi');
      // Now destroy
      await ws.destroy();

      expect(CodeDaemon.shellDestroy).toHaveBeenCalledWith(HANDLE);
      expect(WorkspaceStrategy.cleanup).toHaveBeenCalledWith(HANDLE, undefined);
    });

    it('destroy skips shell cleanup if no shell was created', async () => {
      vi.mocked(WorkspaceStrategy.cleanup).mockResolvedValue();

      const ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);
      await ws.destroy();

      expect(CodeDaemon.shellDestroy).not.toHaveBeenCalled();
      expect(WorkspaceStrategy.cleanup).toHaveBeenCalledWith(HANDLE, undefined);
    });
  });

  describe('multi-workspace isolation', () => {
    it('two workspaces from different create calls have independent handles', async () => {
      vi.mocked(WorkspaceStrategy.create)
        .mockResolvedValueOnce({
          handle: 'worktree-persona-room-a',
          workspaceDir: '/tmp/workspace/room-a',
          mode: 'worktree',
          branch: 'ai/helper/room-a',
        })
        .mockResolvedValueOnce({
          handle: 'worktree-persona-room-b',
          workspaceDir: '/tmp/workspace/room-b',
          mode: 'worktree',
          branch: 'ai/helper/room-b',
        });

      const wsA = await Workspace.create({ personaId: PERSONA_ID, mode: 'worktree', taskSlug: 'room-a' });
      const wsB = await Workspace.create({ personaId: PERSONA_ID, mode: 'worktree', taskSlug: 'room-b' });

      expect(wsA.handle).toBe('worktree-persona-room-a');
      expect(wsB.handle).toBe('worktree-persona-room-b');
      expect(wsA.handle).not.toBe(wsB.handle);
      expect(wsA.dir).not.toBe(wsB.dir);
      expect(wsA.branch).not.toBe(wsB.branch);
    });

    it('operations on workspace A do not affect workspace B', async () => {
      const wsA = Workspace.fromExisting('handle-a', '/tmp/ws-a', 'worktree', 'branch-a');
      const wsB = Workspace.fromExisting('handle-b', '/tmp/ws-b', 'worktree', 'branch-b');

      vi.mocked(CodeDaemon.workspaceRead).mockResolvedValue({} as any);
      vi.mocked(CodeDaemon.workspaceWrite).mockResolvedValue({} as any);

      await wsA.read('file.ts');
      await wsB.write('other.ts', 'content');

      expect(vi.mocked(CodeDaemon.workspaceRead).mock.calls[0][0]).toBe('handle-a');
      expect(vi.mocked(CodeDaemon.workspaceWrite).mock.calls[0][0]).toBe('handle-b');
    });

    it('destroying one workspace does not affect another', async () => {
      vi.mocked(WorkspaceStrategy.cleanup).mockResolvedValue();

      const wsA = Workspace.fromExisting('handle-a', '/tmp/ws-a', 'worktree', 'branch-a');
      const wsB = Workspace.fromExisting('handle-b', '/tmp/ws-b', 'worktree', 'branch-b');

      await wsA.destroy();

      // wsB should still be usable
      vi.mocked(CodeDaemon.workspaceRead).mockResolvedValue({} as any);
      await wsB.read('file.ts');

      expect(WorkspaceStrategy.cleanup).toHaveBeenCalledWith('handle-a', undefined);
      expect(CodeDaemon.workspaceRead).toHaveBeenCalledWith('handle-b', 'file.ts', undefined, undefined);
    });
  });

  describe('handle consistency', () => {
    it('every operation uses the same handle — no handle drift', async () => {
      const ws = Workspace.fromExisting(HANDLE, WORKSPACE_DIR, 'worktree', BRANCH);

      // Call several operations
      vi.mocked(CodeDaemon.workspaceRead).mockResolvedValue({} as any);
      vi.mocked(CodeDaemon.workspaceWrite).mockResolvedValue({} as any);
      vi.mocked(CodeDaemon.workspaceSearch).mockResolvedValue({} as any);
      vi.mocked(CodeDaemon.workspaceGitAdd).mockResolvedValue({ staged: [] });
      vi.mocked(CodeDaemon.workspaceGitCommit).mockResolvedValue({ hash: '' });

      await ws.read('a.ts');
      await ws.write('b.ts', 'content');
      await ws.search('pattern');
      await ws.gitAdd(['.']);
      await ws.gitCommit('msg');

      // Every call should have used the exact same handle
      expect(vi.mocked(CodeDaemon.workspaceRead).mock.calls[0][0]).toBe(HANDLE);
      expect(vi.mocked(CodeDaemon.workspaceWrite).mock.calls[0][0]).toBe(HANDLE);
      expect(vi.mocked(CodeDaemon.workspaceSearch).mock.calls[0][0]).toBe(HANDLE);
      expect(vi.mocked(CodeDaemon.workspaceGitAdd).mock.calls[0][0]).toBe(HANDLE);
      expect(vi.mocked(CodeDaemon.workspaceGitCommit).mock.calls[0][0]).toBe(HANDLE);
    });
  });
});
