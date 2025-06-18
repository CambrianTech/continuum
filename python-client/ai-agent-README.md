# AI Agent Dashboard - Your Command Center

## Quick Start
```bash
# Show everything - rules, tickets, priorities  
python3 ai-agent.py --dashboard

# Focus on what's broken (highest impact)
python3 ai-agent.py --broken

# See what the last agent worked on
python3 ai-agent.py --recent

# Quick status check
python3 ai-agent.py --quick
```

## How to Use This for Debugging

### 1. Check Dashboard First
```bash
python3 ai-agent.py --dashboard
```
- See collaboration rules
- Get current broken commands with specific error traces
- Pick highest priority ticket (🔴 broken > 🟠 untested > 🟡 testing)

### 2. Test the Command 
```bash
# Test to confirm the issue
python3 ai-portal.py --cmd [command-name]

# Get full debug info if needed
python3 ai-portal.py --cmd [command-name] --debug
```

### 3. Use Sentinel for Deep Debugging
```bash
# Start sentinel monitoring (use natural language)
python3 ai-portal.py --cmd sentinel start debug-[command-name]

# Run your test command while sentinel watches
python3 ai-portal.py --cmd [command-name]

# Check what sentinel logged
python3 ai-portal.py --cmd sentinel status

# View the logs manually (sentinel creates organized directories)
ls .continuum/sentinel/debug-[command-name]/
cat .continuum/sentinel/debug-[command-name]/issues-*.log
cat .continuum/sentinel/debug-[command-name]/server-monitor-*.log
```

### 4. Update the Ticket (Always!)
Even if you don't fix it, update the README:
```markdown
## Learning Notes (for next AI agent)
**🔍 Investigation Results (YYYY-MM-DD)**:
- What you found
- Root cause if known  
- Next steps for fixing
- Related commands with same issue
```

### 5. Sync Dashboard
```bash
python3 ai-portal.py --cmd docs
```

## Priority Logic (Built Into Dashboard)

**🔴 BROKEN (Priority 1)** - Work on these first

*Current Top Priorities (as of 2025-06-18):*
1. **input** - Method signature fix identified: change instance to static execute()
2. **cursor, type** - Likely same BaseCommand issue as input  
3. **chat, emotion** - Parameter validation issues, need server-side investigation
4. **reload** - Base64 encoding server validation issue
5. **diagnostics** - Missing test file dependency

*General Pattern Recognition:*
- Method signature mismatches = Quick fix (change instance to static)
- Missing execute methods = Add method implementation  
- Parameter parsing errors = Server-side validation issues

**🟠 UNTESTED (Priority 2)** - Great for exploration  
- No documentation yet
- Unknown functionality
- Good for learning the system

**🟡 TESTING (Priority 3)** - Help finish what others started
- README-driven migration in progress
- Partial implementations

**🟢 STABLE (Priority 4)** - Working well
- Don't break these!
- Good reference examples

## Filtering & Sorting Options
```bash
# Custom views
python3 ai-agent.py --broken --sort name
python3 ai-agent.py --broken --sort date  
python3 ai-agent.py --broken --limit 3

# Filter by status type
python3 ai-agent.py --filter broken
python3 ai-agent.py --filter untested
```

## Debugging Workflow with Logs

### For Client Issues:
1. Check `.continuum/ai-portal/logs/buffer.log`
2. Use sentinel to monitor while testing
3. Look for JavaScript console errors in browser

### For Server Issues:
1. Check server console output
2. Use sentinel to capture server logs
3. Look for command execution errors

### For Command Issues:
1. Test command directly with `--debug`
2. Check command's README for known issues
3. Look at command source code in `src/commands/core/[command]/`

## Common Patterns We've Found

**Instance vs Static Methods:**
- Symptom: "execute must be implemented by subclass"
- Fix: Change `async execute()` to `static async execute()` 
- Examples: `input`, `cursor`, `type` commands

**Parameter Parsing:**
- Symptom: Server validation errors
- Fix: Check parameter format and validation rules
- Examples: `chat`, `emotion` commands

**Missing Dependencies:**
- Symptom: "Cannot read properties of undefined"
- Fix: Check what the command expects to be initialized
- Examples: `diagnostics` missing test files

## Tips for Success

1. **Start Small**: Pick one 🔴 command, investigate 5-10 minutes
2. **Document Everything**: Your notes help the next person
3. **Test Changes**: Always verify fixes work
4. **Use Logs**: Sentinel and debug output are your friends
5. **Follow the Breadcrumbs**: Check what the last agent learned

Remember: Even 5 minutes of investigation and documentation helps the whole team!