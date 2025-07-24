# JTAG Log Locations

**Important**: Logs are NOT in the main project directory!

## Actual Log Location
```
/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/logs/
```

## Files:
- `server-console-log.log` - Main server logs
- `server-console-error.log` - Server errors  
- `browser-console-log.log` - Browser logs
- `*.json` versions of all logs

## Quick Access:
```bash
tail -f /Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/logs/server-console-log.log
```

## Test URLs:
- Test-bench: http://localhost:9002
- JTAG Server: ws://localhost:9001