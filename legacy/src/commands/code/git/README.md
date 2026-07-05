# code/git

Workspace-scoped git operations for the coding agent pipeline. All operations route through the Rust IPC backend for per-persona workspace isolation.

## Operations

| Operation | Description | Required Params |
|-----------|-------------|-----------------|
| `status` | Show workspace git status | - |
| `diff` | Show uncommitted changes | `staged?` |
| `log` | Show recent commits | `count?` |
| `add` | Stage files for commit | `paths` |
| `commit` | Create a commit | `message` |
| `push` | Push to remote | `remote?`, `branch?` |

## Usage

```bash
# Check workspace status
./jtag code/git --userId="persona-id" --operation=status

# View changes
./jtag code/git --userId="persona-id" --operation=diff
./jtag code/git --userId="persona-id" --operation=diff --staged=true

# View history
./jtag code/git --userId="persona-id" --operation=log --count=5

# Stage and commit
./jtag code/git --userId="persona-id" --operation=add --paths='["."]'
./jtag code/git --userId="persona-id" --operation=commit --message="Add feature"

# Push (requires system tier in coding pipeline)
./jtag code/git --userId="persona-id" --operation=push
```

## Security Tiers

- `status`, `diff`, `log`: Read tier (read-only operations)
- `add`, `commit`: Write tier (modifies repository state)
- `push`: Write tier via CLI; system tier when used in coding pipeline plans

## Programmatic Usage

```typescript
import { CodeGit } from './shared/CodeGitTypes';

const status = await CodeGit.execute({ userId: 'persona-id', operation: 'status' });
console.log(status.status?.branch, status.status?.modified);
```
