# JTAG Archive Directory - Scheduled for Deletion

**This directory contains old test files and documentation pollution from the jtag codebase.**

## What's Here

- `test-*.ts`, `test-*.sh`, `test-*.js` - Old adhoc test scripts (superseded by proper tests/)
- `test-*.jsonl` - Old test datasets
- `old-docs-archive/` - Old validation session files and system fix plans
- `joel-widget-thoughts.txt` - Personal development notes
- `create_response.json` - Leftover API response file

## Why It's Here

These files cluttered the jtag root directory. Modern JTAG uses:
- `tests/` - Proper test suite (unit, integration, system tests)
- `.continuum/sessions/validation/` - Validation session records
- Clean root with only essential config files

## Action Required

**This directory should be DELETED after confirming nothing critical is lost.**

Review checklist:
- [ ] Verified no active references to code in `_archive/`
- [ ] Confirmed test coverage exists in proper `tests/` directory
- [ ] Checked no critical documentation only exists here
- [ ] Ready to delete: `git rm -rf _archive/ && git commit -m "Remove jtag archived test pollution"`

**Expected deletion date**: Within 1-2 weeks of 2025-11-05

If you need something from here, move it to the appropriate modern location first, then delete this directory.
