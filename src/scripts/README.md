# Helper Scripts

## Documentation Commits

Documentation-only changes still use normal git hooks.

**Purpose**: Keep docs fast to validate without creating a bypass culture.
Run focused docs checks before committing, then commit normally so the repository
uses the same validation path for humans and agents.

`--no-verify` is forbidden. If hooks fail on a docs-only change because a
worktree is stale, fix that worktree, dependency, submodule, generated-file, or
hook problem instead of bypassing validation.

### Usage

```bash
npx markdownlint-cli2 "docs/**/*.md"
git diff --check
git add docs/path/to-file.md
git commit -m "docs: update architecture note"
```

### Example

```bash
# Good: Only documentation changed
npx markdownlint-cli2 docs/architecture/PERSONA-AS-RUST-LIBRARY-PLAN.md
git diff --check
git commit -m "docs: update PersonaUser architecture"

# Rejected by review/process: any command that bypasses git hooks
```

### Allowed File Types

- Markdown (`.md`)
- Text files (`.txt`)
- README, LICENSE, CHANGELOG
- Shell scripts (`.sh`, `scripts/` directory)
- ReStructuredText (`.rst`)
- AsciiDoc (`.adoc`)

### When to Use Focused Docs Checks

✅ **Run focused docs checks when**:
- Adding or updating documentation
- Writing architecture design docs
- Adding shell helper scripts
- Updating READMEs or CHANGELOGs

❌ **Run the full relevant validation when**:
- Changing any code files (.ts, .js, .tsx)
- Updating package.json or package-lock.json
- Mixed documentation + code changes
- Any changes that should run tests

### Benefits

- **Fast local signal**: Markdown lint and whitespace checks catch doc
  mistakes before hooks.
- **Same validation path**: Normal git hooks still run.
- **No hidden escape hatch**: Agents cannot silently skip validation for convenience.
