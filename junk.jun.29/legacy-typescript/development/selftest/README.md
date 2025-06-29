# SelfTest Command

JTAG system verification using the existing daemon sessions and sandboxing infrastructure.

## Purpose

Verifies system health by testing the JTAG stimulus-response system using the built-in daemon capabilities:
- Session management and sandboxing
- Browser coordination 
- Console logging (JTAG)
- Screenshot capture

## Usage

```bash
# Full system test
continuum selftest

# Test specific scope
continuum selftest --scope=jtag
continuum selftest --scope=session  
continuum selftest --scope=browser

# Verbose output
continuum selftest --verbose=true
```

## Integration

Perfect for git hooks - replaces the old hacky verification with clean daemon-based testing:

```bash
# In git pre-commit hook
python3 python-client/ai-portal.py --cmd selftest
```

## Tests

- **JTAG Logging**: Stimulus-response via console.log verification
- **Session Management**: Daemon session isolation and coordination
- **Browser Coordination**: Screenshot capture and browser control

Uses the existing intelligent system instead of creating external verification scripts.