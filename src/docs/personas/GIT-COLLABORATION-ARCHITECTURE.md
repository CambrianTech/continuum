# Git-Based Collaboration Architecture

## Vision

Enable AI personas to collaboratively write docs and code using standard git workflows, with each persona having their own isolated workspace and git identity.

**Immediate use case:** Collaborative document writing (Decision Intelligence MVP)
**Long-term vision:** AIs writing code together across the entire system

---

## Persona Workspace Structure

```
.continuum/sessions/user/shared/{persona-id}/
  ├── logs/
  ├── state/
  └── workspace/               # Git-enabled workspace
      ├── .git/                # Local git config with persona identity
      ├── docs/                # Files checked out for editing
      └── src/                 # (Future: code files)
```

---

## Implementation Strategy: Git Worktrees

**Why worktrees?**
- Minimal: Only checked-out files exist in persona workspace
- Shared history: All worktrees share the main repo's .git database
- Isolated branches: Each persona works on their own branch
- Native git: Uses built-in git functionality, no custom logic

**Architecture:**
```
Main repo: /Volumes/FlashGordon/cambrian/continuum/
Worktrees:
  - .continuum/sessions/.../deepseek-id/workspace/  (worktree on branch deepseek/section-03)
  - .continuum/sessions/.../claude-id/workspace/    (worktree on branch claude/section-01)
  - .continuum/sessions/.../helper-id/workspace/    (worktree on branch helper/bugfix-123)
```

---

## Persona Git Identity

Each persona's workspace has local git config:

```bash
# In .continuum/sessions/.../persona-id/workspace/.git/config
[user]
    name = DeepSeek Assistant
    email = deepseek@continuum.local
```

**Commits from this workspace automatically use persona identity.**

---

## Commands

### `git/workspace/init`
**Purpose:** Initialize persona's git workspace (creates worktree)

**Parameters:**
- `branch?` - Branch name (defaults to `{persona-name}/{timestamp}`)

**What it does:**
1. Creates worktree at `.continuum/sessions/.../persona-id/workspace/`
2. Creates new branch from main
3. Sets local git config with persona identity
4. Returns workspace path

**Example:**
```bash
./jtag git/workspace/init --branch="section-03"
# Creates: .continuum/sessions/.../deepseek-id/workspace/ on branch deepseek/section-03
```

---

### `git/checkout`
**Purpose:** Check out specific files into workspace

**Parameters:**
- `files` - Array of file paths to check out from main

**What it does:**
1. Ensures workspace exists (calls init if needed)
2. Copies specified files from main branch
3. Returns list of files in workspace

**Example:**
```bash
./jtag git/checkout --files='["docs/decision-intelligence-mvp.md"]'
# File appears in workspace, ready to edit
```

---

### `git/commit`
**Purpose:** Commit changes in workspace with persona identity

**Parameters:**
- `message` - Commit message
- `files?` - Specific files to commit (defaults to all)

**What it does:**
1. Stages specified files (or all changes)
2. Commits with persona's git identity (automatic from workspace config)
3. Returns commit hash and summary

**Example:**
```bash
./jtag git/commit --message="docs: Add technical design section"
# Committed as "DeepSeek Assistant <deepseek@continuum.local>"
```

---

### `git/push`
**Purpose:** Push persona's branch to main repo and optionally merge

**Parameters:**
- `merge?` - Auto-merge to main if no conflicts (default: false)
- `createPR?` - Create GitHub PR instead of direct merge (default: false)

**What it does:**
1. Pushes persona's branch to main repo
2. If `merge=true`: Attempts `git merge` to main (fails if conflicts)
3. If `createPR=true`: Uses `gh pr create` to create GitHub PR
4. Returns merge status or PR URL

**Example:**
```bash
# Push and auto-merge
./jtag git/push --merge=true

# Push and create PR for review
./jtag git/push --createPR=true --title="Add technical design section"
```

---

### `git/status`
**Purpose:** Show workspace state

**Returns:**
- Modified files
- Uncommitted changes
- Branch name
- Commits ahead of main

**Example:**
```bash
./jtag git/status
# Output: "Branch: deepseek/section-03 | Modified: 1 file | Uncommitted: yes"
```

---

### `git/diff`
**Purpose:** Show changes in workspace

**Parameters:**
- `files?` - Specific files to diff (defaults to all)

**Returns:**
- Git diff output

---

### `git/workspace/clean`
**Purpose:** Clean up workspace (remove worktree)

**Parameters:**
- `force?` - Force cleanup even with uncommitted changes

**What it does:**
1. Removes worktree directory
2. Deletes branch if merged
3. Cleans up git references

---

## Workflow Example: Collaborative Doc Writing

**Scenario:** DeepSeek wants to write section 03 of Decision Intelligence MVP

```bash
# 1. Initialize workspace and create branch
./jtag git/workspace/init --branch="section-03"

# 2. Check out the doc file
./jtag git/checkout --files='["docs/decision-intelligence-mvp.md"]'

# 3. Edit the file in workspace
# (AI uses Write/Edit tools on workspace file)

# 4. Check status before committing
./jtag git/status

# 5. Commit changes
./jtag git/commit --message="docs: Add technical design section"

# 6. Push and merge to main
./jtag git/push --merge=true

# 7. Clean up workspace
./jtag git/workspace/clean
```

**Result:**
- Commit shows "DeepSeek Assistant" as author
- Changes merged to main automatically (no conflicts)
- Other AIs can now see the changes

---

## Conflict Resolution

**If merge fails due to conflicts:**

```bash
./jtag git/push --merge=true
# Error: "Merge conflict in docs/decision-intelligence-mvp.md"

# Option 1: Create PR for manual resolution
./jtag git/push --createPR=true --title="Add section 03 (has conflicts)"

# Option 2: Pull latest main and retry
./jtag git/pull --from=main
# Resolve conflicts in workspace
./jtag git/commit --message="Merge main and resolve conflicts"
./jtag git/push --merge=true
```

---

## Security & Isolation

**Git credentials:**
- Persona workspaces use Joel's SSH keys for push (inherit from parent process)
- No need for persona-specific GitHub accounts
- Git commits show persona identity, but pushes use Joel's auth

**File system isolation:**
- Each workspace is isolated in persona's session directory
- No cross-contamination between persona workspaces
- Main repo remains untouched until explicit merge

---

## Extension to Code Collaboration

**Same workflow applies to code:**

```bash
# AI wants to implement feature
./jtag git/workspace/init --branch="add-rate-limiting"
./jtag git/checkout --files='["src/services/api.ts", "tests/api.test.ts"]'

# AI edits files, writes tests
./jtag git/commit --message="feat: Add rate limiting to API"

# Create PR for human review
./jtag git/push --createPR=true --title="Add rate limiting"
```

**Future enhancements:**
- Pre-commit hooks run tests before commit
- Automated code review by other AIs
- Parallel feature development by multiple AIs
- Merge queues for CI/CD integration

---

## Implementation Plan

1. **Phase 1:** Build core commands (init, checkout, commit, push, status)
2. **Phase 2:** Test with Decision Intelligence doc (real use case)
3. **Phase 3:** Add conflict resolution helpers (pull, merge)
4. **Phase 4:** Add PR workflow (createPR, review)
5. **Phase 5:** Extend to code collaboration with pre-commit hooks

---

## Benefits Over Custom Solutions

**Leverages battle-tested technology:**
- Git's merge algorithms (20+ years of development)
- Standard workflows (branch, commit, merge, PR)
- Rich tooling ecosystem (diff, log, blame, etc.)

**Zero custom maintenance:**
- No custom conflict resolution logic
- No custom attribution tracking
- No custom versioning system

**Scales naturally:**
- Works for 2 collaborators or 200
- Extends from docs to code to any text files
- Integrates with existing CI/CD pipelines

---

## Open Questions

1. **Workspace lifecycle:** Persistent or ephemeral (created per task)?
   - **Proposal:** Ephemeral - create on init, clean after push

2. **Auto-merge policy:** Always auto-merge or require review?
   - **Proposal:** Auto-merge for docs, PR review for code

3. **Branch naming:** Strict convention or free-form?
   - **Proposal:** `{persona-name}/{feature-or-section}` (e.g., `deepseek/section-03`)

4. **Conflict resolution:** Who resolves conflicts when auto-merge fails?
   - **Proposal:** Create PR, let human or senior AI resolve

5. **Cleanup timing:** Immediate after merge or lazy cleanup?
   - **Proposal:** Immediate cleanup after successful merge, keep on conflict
