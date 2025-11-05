# Archive Directory - Scheduled for Deletion

**This directory contains old/deprecated code that polluted the root directory.**

## What's Here

- `agent-scripts/` - Old diagnostic/healing scripts (pre-JTAG era)
- `agents/` - Old agent workspace code
- `archived-systems/` - Previously archived systems
- `design/` - Old design docs (superseded by src/debug/jtag/design/)
- `middle-out/` - Old middle-out mining analysis
- `verification_system/`, `verification/` - Old verification systems
- `examples/` - Old example scripts
- `templates/` - Old template files
- `system/` - Old system architecture attempts
- `old-archive/` - Previous archive directory

## Why It's Here

These directories cluttered the root and made the repo structure unclear. Modern Continuum uses:
- `src/` - Source code (main codebase in src/debug/jtag/)
- `papers/` - Research papers
- `docs/` - Architecture documentation
- `scripts/` - Build/deployment scripts
- `screenshots/` - UI screenshots
- `python-client/` - Python client code

## Action Required

**This directory should be DELETED after confirming nothing critical is lost.**

Review checklist:
- [ ] Verified no active references to code in `_archive/`
- [ ] Confirmed modern equivalents exist for any useful code
- [ ] Checked no critical documentation only exists here
- [ ] Ready to delete: `git rm -rf _archive/ && git commit -m "Remove archived pollution"`

**Expected deletion date**: Within 1-2 weeks of 2025-11-05

If you need something from here, move it to the appropriate modern location first, then delete this directory.
