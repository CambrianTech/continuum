# Git Commands Roadmap

**Vision**: Complete git and GitHub integration through JTAG commands, enabling the AI team to discover bugs, create issues, submit PRs, and participate in the full development lifecycle.

---

## Why This Matters

The AI team (13 PersonaUsers + 50+ external AIs) actively explore and test the system. When they discover bugs (like the `data/list` replace() error Grok found), they currently can't:
- Create GitHub issues automatically
- Track bug status
- Link bugs to commits
- Submit fixes as PRs

**With git commands, the AI team becomes a full QA + development force.**

---

## Phase 1: Issue Tracking (P0 - Implementing First)

### git/issue/create
**Purpose**: Let AIs file bugs they discover

**Usage**:
```bash
./jtag git/issue/create \
  --title="data/list replace() error" \
  --body="Command fails with: Cannot read properties of undefined (reading 'replace')

**Location**: cli.ts:163
**Discovered by**: @grok
**Test case**: ./jtag data/list --collection=users" \
  --labels="bug,P0,ai-discovered" \
  --assignee="joelonsoftware"
```

**Returns**:
```json
{
  "success": true,
  "issue": {
    "number": 123,
    "url": "https://github.com/user/repo/issues/123",
    "title": "data/list replace() error",
    "state": "open"
  }
}
```

**Implementation**: Uses `gh issue create` under the hood

---

### git/issue/list
**Purpose**: Browse open issues

**Usage**:
```bash
# List all open bugs
./jtag git/issue/list --state=open --label=bug

# List P0 issues assigned to me
./jtag git/issue/list --label=P0 --assignee=@me

# List AI-discovered bugs
./jtag git/issue/list --label=ai-discovered --limit=50
```

**Returns**:
```json
{
  "success": true,
  "issues": [
    {
      "number": 123,
      "title": "data/list replace() error",
      "state": "open",
      "labels": ["bug", "P0", "ai-discovered"],
      "assignee": "joelonsoftware",
      "createdAt": "2025-11-19T23:00:00Z",
      "updatedAt": "2025-11-19T23:10:00Z"
    }
  ],
  "count": 1
}
```

**Implementation**: Uses `gh issue list` with filters

---

### git/issue/get
**Purpose**: Get detailed info on a specific issue

**Usage**:
```bash
./jtag git/issue/get --number=123
```

**Returns**:
```json
{
  "success": true,
  "issue": {
    "number": 123,
    "title": "data/list replace() error",
    "body": "Full issue description...",
    "state": "open",
    "labels": ["bug", "P0"],
    "assignee": "joelonsoftware",
    "comments": 3,
    "createdAt": "2025-11-19T23:00:00Z"
  }
}
```

---

### git/issue/comment
**Purpose**: Add updates to issues

**Usage**:
```bash
./jtag git/issue/comment \
  --number=123 \
  --body="✅ Fixed in commit abc123. Changed arg?.startsWith() to arg && arg.startsWith()"
```

**Use Case**: AIs can provide updates when they discover more details or when a fix is deployed

---

### git/issue/update
**Purpose**: Close/reopen issues, update labels

**Usage**:
```bash
# Close an issue
./jtag git/issue/update --number=123 --state=closed

# Reopen an issue
./jtag git/issue/update --number=123 --state=open

# Add labels
./jtag git/issue/update --number=123 --add-labels="needs-testing,regression"

# Change assignee
./jtag git/issue/update --number=123 --assignee="@claude"
```

---

## Phase 2: Pull Requests

### git/pr/create
**Purpose**: Create pull requests

**Usage**:
```bash
./jtag git/pr/create \
  --title="Fix: data/list replace() error" \
  --body="Fixes #123

Changed cli.ts:162 from optional chaining to proper null check.

**Testing**: ./jtag data/list --collection=users works without errors" \
  --base=main \
  --head=fix/data-list-error \
  --labels="bug,fix"
```

---

### git/pr/list
**Purpose**: Browse pull requests

**Usage**:
```bash
# List open PRs
./jtag git/pr/list --state=open

# List PRs by author
./jtag git/pr/list --author="@me"

# List PRs that fix bugs
./jtag git/pr/list --label=bug
```

---

### git/pr/get
**Purpose**: Get detailed PR info

**Usage**:
```bash
./jtag git/pr/get --number=456
```

---

### git/pr/merge
**Purpose**: Merge pull requests

**Usage**:
```bash
# Squash merge (default)
./jtag git/pr/merge --number=456 --method=squash

# Regular merge
./jtag git/pr/merge --number=456 --method=merge

# Rebase merge
./jtag git/pr/merge --number=456 --method=rebase
```

---

### git/pr/review
**Purpose**: Review and approve PRs

**Usage**:
```bash
# Approve
./jtag git/pr/review --number=456 --approve

# Request changes
./jtag git/pr/review --number=456 --request-changes \
  --body="Please add tests for the fix"

# Comment
./jtag git/pr/review --number=456 --comment \
  --body="Looks good, just needs docs"
```

---

## Phase 3: Commits & Repository

### git/commit/create
**Purpose**: Create commits with standardized messages

**Usage**:
```bash
./jtag git/commit/create \
  --message="Fix: data/list replace() error

Changed cli.ts:162 from optional chaining to proper null check.

Fixes #123" \
  --files="cli.ts"
```

---

### git/commit/template
**Purpose**: Get commit message templates

**Usage**:
```bash
# Get bugfix template
./jtag git/commit/template --type=bugfix

# Returns:
# Fix: [short description]
#
# [Detailed description of the fix]
#
# Fixes #[issue-number]
```

---

### git/status
**Purpose**: Get repository status

**Usage**:
```bash
./jtag git/status
```

**Returns**:
```json
{
  "success": true,
  "branch": "main",
  "modified": ["cli.ts"],
  "untracked": ["test.ts"],
  "ahead": 2,
  "behind": 0
}
```

---

### git/log
**Purpose**: View commit history

**Usage**:
```bash
# Recent commits
./jtag git/log --limit=10

# Commits for a file
./jtag git/log --file="cli.ts" --limit=5

# Commits by author
./jtag git/log --author="@claude" --limit=20
```

---

### git/diff
**Purpose**: View file differences

**Usage**:
```bash
# Diff for a file
./jtag git/diff --file="cli.ts"

# Diff between commits
./jtag git/diff --from="abc123" --to="def456"

# Staged changes
./jtag git/diff --staged
```

---

### git/blame
**Purpose**: See who changed what line

**Usage**:
```bash
./jtag git/blame --file="cli.ts" --line=162
```

**Returns**:
```json
{
  "success": true,
  "commit": "abc123",
  "author": "Joel",
  "date": "2025-11-19",
  "line": 162,
  "content": "if (arg && arg.startsWith('--')) {"
}
```

---

## Integration with Existing Systems

### Auto-Issue Creation from Errors

When an AI discovers a bug, they can:
1. Report it in chat (current behavior)
2. **Automatically create a GitHub issue**:

```typescript
// In PersonaUser when encountering an error:
const issueResult = await Commands.execute('git/issue/create', {
  title: `${commandName} error: ${errorMessage}`,
  body: `**Error**: ${error.stack}

**Discovered by**: ${this.displayName}
**Timestamp**: ${new Date().toISOString()}
**Command**: ${commandName}
**Parameters**: ${JSON.stringify(params, null, 2)}`,
  labels: ['bug', 'ai-discovered', 'auto-generated']
});

// Comment in chat with issue link
await this.sendMessage(`I encountered an error and created issue #${issueResult.issue.number}: ${issueResult.issue.url}`);
```

---

### Link Issues to Commits

When committing a fix:
```bash
./jtag git/commit/create \
  --message="Fix: data/list replace() error

Fixes #123" \
  --files="cli.ts"
```

GitHub automatically links the commit to issue #123 and closes it when merged.

---

### Integration with BUGFIXES Document

Current workflow:
1. AIs discover bugs → Joel manually documents in `BUGFIXES-2025-11-19.md`
2. Joel fixes bugs → manually updates document

**New workflow**:
1. AIs discover bugs → auto-create GitHub issues
2. Joel/AIs fix bugs → commits auto-link to issues
3. Generate BUGFIXES doc from GitHub:

```bash
./jtag git/issue/list --label=bug --state=closed --format=markdown > BUGFIXES-$(date +%Y-%m-%d).md
```

---

## AI Team Suggestions (From Chat)

Based on feedback from the AI team:

### Claude Assistant:
- Issue tracking to create, read, update issues
- Git repository inspection (history, diffs, branches)
- Test execution access
- Log analysis tools
- Link errors to issues and commits

### Grok:
- Full git repository access (clone or read-only)
- API access to issue trackers (GitHub Issues/Jira)
- Hook into existing tools like `code/read` and `exec`
- Automate bug triage based on patterns

### DeepSeek Assistant:
- Codebase structure access
- Issue tracking integration
- Git repository access
- Check data handles and collections

### Fireworks AI & Together Assistant:
- Access to codebase (especially issue tracker parts)
- Architecture documentation
- Existing APIs and tools documentation

### Groq Lightning:
- Git repository access via `data/open`
- Schema access via `data/schema`
- Ecosystem documentation via `ai/report`
- Issue tracker integration reporting

---

## Implementation Priority

**Phase 1A** (This Week):
1. ✅ Document roadmap (this file)
2. `git/issue/create` - Let AIs file bugs
3. `git/issue/list` - Browse issues
4. Test with AI team

**Phase 1B** (Next Week):
5. `git/issue/comment` - Add updates
6. `git/issue/update` - Close/manage issues
7. `git/issue/get` - Get details

**Phase 2** (Following Sprint):
8. PR creation and management commands
9. Integration with AI workflow automation

**Phase 3** (Future):
10. Commit management
11. Repository inspection
12. Git history analysis

---

## Technical Implementation

### Command Structure (Following Existing Patterns)

```typescript
// commands/git/issue/create/shared/GitIssueCreateTypes.ts
export interface GitIssueCreateParams extends JTAGPayload {
  readonly title: string;
  readonly body: string;
  readonly labels?: string[];
  readonly assignee?: string;
  readonly milestone?: number;
  readonly repo?: string; // Defaults to current repo
}

export interface GitIssueCreateResult extends JTAGPayload {
  readonly success: boolean;
  readonly issue?: {
    readonly number: number;
    readonly url: string;
    readonly title: string;
    readonly state: string;
  };
  readonly error?: string;
}
```

### Server Implementation

```typescript
// commands/git/issue/create/server/GitIssueCreateServerCommand.ts
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class GitIssueCreateServerCommand extends CommandBase<GitIssueCreateParams, GitIssueCreateResult> {
  async execute(params: GitIssueCreateParams): Promise<GitIssueCreateResult> {
    try {
      // Build gh CLI command
      const labels = params.labels?.join(',') || '';
      const assignee = params.assignee || '';

      const cmd = `gh issue create \
        --title "${params.title}" \
        --body "${params.body}" \
        ${labels ? `--label "${labels}"` : ''} \
        ${assignee ? `--assignee "${assignee}"` : ''} \
        --json number,url,title,state`;

      const { stdout } = await execAsync(cmd);
      const issue = JSON.parse(stdout);

      return {
        ...params,
        success: true,
        issue: {
          number: issue.number,
          url: issue.url,
          title: issue.title,
          state: issue.state
        }
      };
    } catch (error) {
      return {
        ...params,
        success: false,
        error: error.message
      };
    }
  }
}
```

### Error Handling

All git commands should:
1. Check if `gh` CLI is installed
2. Check if in a git repository
3. Check for GitHub authentication
4. Provide helpful error messages

```typescript
// Reusable validation
async function validateGitEnvironment(): Promise<{ valid: boolean; error?: string }> {
  // Check gh CLI
  try {
    await execAsync('which gh');
  } catch {
    return { valid: false, error: 'GitHub CLI (gh) not installed. Install: brew install gh' };
  }

  // Check auth
  try {
    await execAsync('gh auth status');
  } catch {
    return { valid: false, error: 'Not authenticated with GitHub. Run: gh auth login' };
  }

  return { valid: true };
}
```

---

## Testing Strategy

### Manual Testing
```bash
# Test issue creation
./jtag git/issue/create --title="Test Issue" --body="Testing the command" --labels="test"

# Verify on GitHub
gh issue list --label=test

# Test listing
./jtag git/issue/list --label=test

# Test closing
./jtag git/issue/update --number=XXX --state=closed
```

### AI Team Testing

Once Phase 1A is complete:
1. Deploy commands
2. Ask AI team to test by creating real bug reports
3. Monitor chat for feedback
4. Iterate based on their experience

---

## Success Metrics

1. **AI-Created Issues**: 10+ issues created by AIs in first week
2. **Bug Discovery Rate**: Increased from manual reporting to automated
3. **Issue Quality**: Issues contain all necessary debug info
4. **Response Time**: Time from bug discovery to issue creation < 1 minute
5. **AI Engagement**: 5+ different AIs actively using the commands

---

## Future Enhancements

### Auto-Triage
AIs analyze new issues and:
- Assign priority labels based on impact
- Suggest which developer to assign
- Link to similar past issues
- Recommend potential fixes

### Pattern Recognition
System learns from bug patterns:
- "Replace() errors usually fixed by null checks"
- "orderBy errors often related to serialization"
- Suggest preventive measures

### Integration with Training
Bugs discovered → training data for genome system:
- Fine-tune models on bug detection
- Improve QA capabilities over time
- Build specialized "Bug Hunter" personas

---

## Related Documents

- [TypeScript Commands Roadmap](../../system/typescript/TYPESCRIPT-COMMANDS-ROADMAP.md)
- [Bug Fixes 2025-11-19](../../BUGFIXES-2025-11-19.md)
- [Universal Primitives](../../docs/UNIVERSAL-PRIMITIVES.md)
- [Commands Architecture](../../docs/ARCHITECTURE-RULES.md)
