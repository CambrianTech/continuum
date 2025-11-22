# Git Commands Implementation Plan
**Status**: Ready to implement
**Priority**: P0 (AI team unanimous vote)
**Estimated Time**: 2-3 hours for Phase 1A

---

## Phase 1A: Core Issue Commands (NEXT)

### 1. `git/issue/create`

**Command Structure**:
```
commands/git/issue/create/
├── shared/
│   └── GitIssueCreateTypes.ts
├── server/
│   └── GitIssueCreateServerCommand.ts
└── browser/
    └── GitIssueCreateBrowserCommand.ts (pass-through to server)
```

**Types** (`shared/GitIssueCreateTypes.ts`):
```typescript
import type { JTAGPayload } from 'system/core/shared/JTAGTypes';

export interface GitIssueCreateParams extends JTAGPayload {
  readonly title: string;
  readonly body: string;
  readonly labels?: string[];  // e.g., ["bug", "P0", "ai-discovered"]
  readonly assignee?: string;  // GitHub username
  readonly milestone?: number; // Milestone number
  readonly repo?: string;      // Defaults to current repo
}

export interface GitIssueCreateResult extends JTAGPayload {
  readonly success: boolean;
  readonly issue?: {
    readonly number: number;
    readonly url: string;
    readonly title: string;
    readonly state: string;
    readonly labels: string[];
  };
  readonly error?: string;
}
```

**Server Implementation** (`server/GitIssueCreateServerCommand.ts`):
```typescript
import { exec } from 'child_process';
import { promisify } from 'util';
import type { GitIssueCreateParams, GitIssueCreateResult } from '../shared/GitIssueCreateTypes';
import { CommandBase } from 'system/core/shared/CommandBase';

const execAsync = promisify(exec);

export class GitIssueCreateServerCommand extends CommandBase<GitIssueCreateParams, GitIssueCreateResult> {
  async execute(params: GitIssueCreateParams): Promise<GitIssueCreateResult> {
    try {
      // Validate environment
      const validation = await this.validateGitEnvironment();
      if (!validation.valid) {
        return {
          ...params,
          success: false,
          error: validation.error
        };
      }

      // Build gh CLI command
      const labels = params.labels?.join(',') || '';
      const assignee = params.assignee || '';

      let cmd = `gh issue create --title "${this.escapeShell(params.title)}" --body "${this.escapeShell(params.body)}"`;

      if (labels) {
        cmd += ` --label "${labels}"`;
      }
      if (assignee) {
        cmd += ` --assignee "${assignee}"`;
      }
      if (params.milestone) {
        cmd += ` --milestone ${params.milestone}`;
      }

      cmd += ' --json number,url,title,state,labels';

      console.log(`🐙 GitHub: Creating issue "${params.title}"`);

      const { stdout, stderr } = await execAsync(cmd, {
        cwd: process.cwd(),
        timeout: 30000
      });

      if (stderr) {
        console.warn(`⚠️  GitHub CLI warning: ${stderr}`);
      }

      const issue = JSON.parse(stdout);

      console.log(`✅ GitHub: Created issue #${issue.number}: ${issue.url}`);

      return {
        ...params,
        success: true,
        issue: {
          number: issue.number,
          url: issue.url,
          title: issue.title,
          state: issue.state,
          labels: issue.labels?.map((l: any) => l.name) || []
        }
      };
    } catch (error) {
      console.error(`❌ GitHub: Failed to create issue`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return {
        ...params,
        success: false,
        error: `Failed to create issue: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  private async validateGitEnvironment(): Promise<{ valid: boolean; error?: string }> {
    try {
      // Check gh CLI installed
      await execAsync('which gh');
    } catch {
      return {
        valid: false,
        error: 'GitHub CLI (gh) not installed. Install: brew install gh'
      };
    }

    try {
      // Check authentication
      await execAsync('gh auth status');
    } catch {
      return {
        valid: false,
        error: 'Not authenticated with GitHub. Run: gh auth login'
      };
    }

    return { valid: true };
  }

  private escapeShell(str: string): string {
    return str.replace(/"/g, '\\"').replace(/\$/g, '\\$').replace(/`/g, '\\`');
  }
}
```

**Browser Command** (pass-through):
```typescript
import type { GitIssueCreateParams, GitIssueCreateResult } from '../shared/GitIssueCreateTypes';
import { BrowserCommandBase } from 'system/core/browser/BrowserCommandBase';

export class GitIssueCreateBrowserCommand extends BrowserCommandBase<GitIssueCreateParams, GitIssueCreateResult> {
  // Pass-through to server - no browser-specific logic needed
}
```

---

### 2. `git/issue/list`

**Command Structure**:
```
commands/git/issue/list/
├── shared/
│   └── GitIssueListTypes.ts
├── server/
│   └── GitIssueListServerCommand.ts
└── browser/
    └── GitIssueListBrowserCommand.ts
```

**Types** (`shared/GitIssueListTypes.ts`):
```typescript
export interface GitIssueListParams extends JTAGPayload {
  readonly state?: 'open' | 'closed' | 'all';  // Default: 'open'
  readonly label?: string;      // Filter by label
  readonly assignee?: string;   // Filter by assignee
  readonly limit?: number;      // Default: 30
  readonly repo?: string;       // Defaults to current repo
}

export interface GitIssue {
  readonly number: number;
  readonly title: string;
  readonly state: string;
  readonly url: string;
  readonly labels: string[];
  readonly assignee: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface GitIssueListResult extends JTAGPayload {
  readonly success: boolean;
  readonly issues?: GitIssue[];
  readonly count?: number;
  readonly error?: string;
}
```

**Server Implementation** (`server/GitIssueListServerCommand.ts`):
```typescript
export class GitIssueListServerCommand extends CommandBase<GitIssueListParams, GitIssueListResult> {
  async execute(params: GitIssueListParams): Promise<GitIssueListResult> {
    try {
      // Validate environment
      const validation = await this.validateGitEnvironment();
      if (!validation.valid) {
        return {
          ...params,
          success: false,
          error: validation.error
        };
      }

      // Build gh CLI command
      const state = params.state || 'open';
      const limit = params.limit || 30;

      let cmd = `gh issue list --state ${state} --limit ${limit}`;

      if (params.label) {
        cmd += ` --label "${params.label}"`;
      }
      if (params.assignee) {
        cmd += ` --assignee "${params.assignee}"`;
      }

      cmd += ' --json number,title,state,url,labels,assignees,createdAt,updatedAt';

      console.log(`🐙 GitHub: Listing issues (state=${state}, limit=${limit})`);

      const { stdout, stderr } = await execAsync(cmd, {
        cwd: process.cwd(),
        timeout: 30000
      });

      if (stderr) {
        console.warn(`⚠️  GitHub CLI warning: ${stderr}`);
      }

      const rawIssues = JSON.parse(stdout);

      const issues: GitIssue[] = rawIssues.map((issue: any) => ({
        number: issue.number,
        title: issue.title,
        state: issue.state,
        url: issue.url,
        labels: issue.labels?.map((l: any) => l.name) || [],
        assignee: issue.assignees?.[0]?.login || null,
        createdAt: issue.createdAt,
        updatedAt: issue.updatedAt
      }));

      console.log(`✅ GitHub: Found ${issues.length} issue(s)`);

      return {
        ...params,
        success: true,
        issues,
        count: issues.length
      };
    } catch (error) {
      console.error(`❌ GitHub: Failed to list issues`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return {
        ...params,
        success: false,
        error: `Failed to list issues: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  private async validateGitEnvironment(): Promise<{ valid: boolean; error?: string }> {
    // Same as GitIssueCreateServerCommand
  }
}
```

---

## Implementation Steps

### Step 1: Create Directory Structure
```bash
mkdir -p commands/git/issue/create/{shared,server,browser}
mkdir -p commands/git/issue/list/{shared,server,browser}
```

### Step 2: Implement Types (shared/)
- `GitIssueCreateTypes.ts`
- `GitIssueListTypes.ts`

### Step 3: Implement Server Commands
- `GitIssueCreateServerCommand.ts`
- `GitIssueListServerCommand.ts`
- Shared validation utility for both

### Step 4: Implement Browser Pass-through Commands
- Simple pass-through to server

### Step 5: Register Commands
- Commands auto-register via structure generator
- Run `npm run build` to regenerate command registry

### Step 6: Test Manually
```bash
# Test create
./jtag git/issue/create \
  --title="Test Issue from JTAG" \
  --body="Testing git/issue/create command" \
  --labels="test,automated"

# Test list
./jtag git/issue/list --state=open --limit=10

# Test filtering
./jtag git/issue/list --label="bug" --state=open
```

### Step 7: Deploy and Notify AI Team
```bash
npm start  # Wait 90 seconds
./jtag chat/send --room="general" --message="✅ git/issue/create and git/issue/list are now live! Try them out."
```

---

## Testing Checklist

### Validation Tests
- [ ] `gh` CLI not installed → helpful error
- [ ] Not authenticated → helpful error with instructions
- [ ] Not in git repo → appropriate error

### Functionality Tests
- [ ] Create issue with title + body
- [ ] Create issue with labels
- [ ] Create issue with assignee
- [ ] Create issue with milestone
- [ ] List open issues
- [ ] List closed issues
- [ ] List all issues
- [ ] Filter by label
- [ ] Filter by assignee
- [ ] Limit results

### Error Handling Tests
- [ ] Invalid label → clear error
- [ ] Invalid assignee → clear error
- [ ] Network timeout → recoverable error
- [ ] GitHub API rate limit → clear message

### Integration Tests
- [ ] AI team can create issues
- [ ] Issues appear on GitHub
- [ ] Issue URLs are clickable
- [ ] Labels are applied correctly

---

## Success Criteria

1. ✅ AI team can run `./jtag git/issue/create` to file bugs
2. ✅ AI team can run `./jtag git/issue/list` to browse issues
3. ✅ Issues created via JTAG appear properly on GitHub
4. ✅ Clear error messages when environment not configured
5. ✅ Commands execute in <5 seconds for typical cases

---

## Future Enhancements (Phase 1B)

After Phase 1A is working:
- `git/issue/comment` - Add comments to issues
- `git/issue/update` - Update issue state, labels, assignee
- `git/issue/get` - Get detailed info on specific issue
- `git/issue/close` - Close an issue with optional comment

---

## Notes

- **Why `gh` CLI?** Simpler than GitHub REST API, handles auth automatically, JSON output built-in
- **Error handling**: All errors should be caught and returned as `{ success: false, error: "..." }`
- **Logging**: Use emoji prefixes (🐙 for GitHub ops, ✅ for success, ❌ for errors)
- **Shell escaping**: Critical for title/body that may contain special chars
- **Timeout**: 30 seconds for GitHub operations (network can be slow)
