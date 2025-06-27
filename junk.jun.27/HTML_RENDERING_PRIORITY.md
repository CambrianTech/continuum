# HTML RENDERING PRIORITY - READ THIS FIRST

## PRIMARY GOAL: GET HTML RENDERING AND WIDGETS WORKING

Everything else is secondary. We need the basic UI system functional.

## CURRENT HTML/WIDGET SYSTEM STATUS

### RendererDaemon (694 lines)
- **Purpose**: Handle all HTML, CSS, static file serving
- **Port**: 9001 for static files
- **Status**: ❌ Needs investigation - likely broken

### WebSocketDaemon (1193 lines) 
- **Problem**: Duplicating RendererDaemon work instead of routing to it
- **Status**: ❌ Doing HTML generation when it should just route

### Widget System
- **Location**: `src/ui/components/*/` 
- **Type**: TypeScript web components
- **Status**: ❌ Unknown if loading properly

## CRITICAL INVESTIGATION NEEDED

1. **Can we load localhost:9000 and see UI?**
2. **Are widgets loading from TypeScript files?**
3. **Is RendererDaemon actually serving static files on 9001?**
4. **Is WebSocketDaemon routing requests to RendererDaemon correctly?**

## IMMEDIATE DEBUGGING STEPS

```bash
# Test if main UI loads
curl http://localhost:9000/

# Test if RendererDaemon is serving static files  
curl http://localhost:9001/src/ui/components/shared/BaseWidget.css

# Check daemon status
npm run test:daemons

# Start system and check logs
python python-client/ai-portal.py --logs 5
```

## HTML RENDERING ARCHITECTURE (CORRECT)

```
Browser Request → WebSocketDaemon:9000 → Routes to → RendererDaemon:9001
                                                   ↓
                                            Serves HTML/CSS/JS
```

## WIDGET LOADING FLOW (EXPECTED)

```
1. Browser loads main HTML from RendererDaemon
2. HTML includes widget-loader.ts 
3. Widget loader discovers TypeScript components
4. Components load CSS from /src/ui/components/*/
5. Components render to DOM
```

## CURRENT FAILURES (LIKELY)

- WebSocketDaemon not routing to RendererDaemon properly
- RendererDaemon not starting HTTP server on 9001
- Widget TypeScript files not compiling/loading
- CSS files not being served with correct headers

## PRIORITY ORDER

1. ✅ Get basic HTML page loading at localhost:9000
2. ✅ Get static file serving working (CSS, JS)  
3. ✅ Get TypeScript widgets loading and rendering
4. ✅ Get widget CSS loading properly
5. Then worry about daemon cleanup

## FILES TO FOCUS ON

- `src/daemons/renderer/RendererDaemon.ts` - Main HTML serving
- `src/integrations/websocket/WebSocketDaemon.ts` - Routing to renderer
- `src/ui/continuum.js` - Legacy JS that loads widgets (convert to TS)
- `src/ui/components/shared/BaseWidget.ts` - Widget foundation

**EVERYTHING ELSE CAN WAIT. GET THE UI WORKING FIRST.**