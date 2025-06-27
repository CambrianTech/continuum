# 🚨 CLAUDE - START HERE - DON'T READ ANYTHING ELSE FIRST 🚨

## YOU ARE IN: /Volumes/FlashGordon/cambrian/continuum

## STOP - DON'T GO ON TANGENTS
You have a tendency to go off on tangents after /compact. FOCUS.

## SINGLE PRIORITY: FIX HTML RENDERING
- localhost:9000 is broken 
- WebSocketDaemon is 1193 lines doing RendererDaemon's job
- Need proper routing: Browser → WebSocketDaemon → RendererDaemon

## TEST FIRST:
```bash
curl http://localhost:9000/
curl http://localhost:9001/src/ui/components/shared/BaseWidget.css
```

## IF BROKEN, FIX:
1. RendererDaemon should serve HTML/CSS/JS
2. WebSocketDaemon should just route to RendererDaemon
3. Delete duplicate HTML generation from WebSocketDaemon

## READ THESE FOR DETAILS:
- `WAKE_UP_READ_THIS.md`
- `HTML_RENDERING_PRIORITY.md` 
- `DAEMON_CLEANUP_STATUS.md`

## DON'T DO ANYTHING ELSE UNTIL HTML WORKS