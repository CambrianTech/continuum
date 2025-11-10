# Helper Scripts

## git-commit-docs.sh

Smart commit script for documentation-only changes that skips the precommit hook.

**Purpose**: When committing only documentation files (markdown, READMEs, etc.), you don't need to run the full precommit hook (which runs TypeScript compilation and tests). This script safely commits documentation-only changes using `--no-verify`.

**Safety**: The script validates that ALL changes are documentation/script files before committing. If any code files (`.ts`, `.js`, `.json`) are detected, it rejects the commit and tells you to use regular `git commit` instead.

### Usage

```bash
./scripts/git-commit-docs.sh "commit message here"
```

### Example

```bash
# Good: Only documentation changed
./scripts/git-commit-docs.sh "docs: update PersonaUser architecture"

# Rejected: Code files detected
./scripts/git-commit-docs.sh "mixed changes"
# ❌ Non-documentation files detected: PersonaUser.ts
# This script is for documentation-only commits.
# Use regular 'git commit' for code changes.
```

### Allowed File Types

- Markdown (`.md`)
- Text files (`.txt`)
- README, LICENSE, CHANGELOG
- Shell scripts (`.sh`, `scripts/` directory)
- ReStructuredText (`.rst`)
- AsciiDoc (`.adoc`)

### When to Use

✅ **Use this script when**:
- Adding or updating documentation
- Writing architecture design docs
- Adding shell helper scripts
- Updating READMEs or CHANGELOGs

❌ **Use regular `git commit` when**:
- Changing any code files (.ts, .js, .tsx)
- Updating package.json or package-lock.json
- Mixed documentation + code changes
- Any changes that should run tests

### Benefits

- **Fast**: Skips 90+ second precommit hook for docs-only changes
- **Safe**: Validates file types before committing
- **Clear**: Color-coded output shows what's being committed
- **Convenient**: Stages all documentation changes automatically
