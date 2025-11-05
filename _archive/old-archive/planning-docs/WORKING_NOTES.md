# Working Notes - Users Section Implementation

## Current Status
- Continuum is running (PID varies, check .continuum/continuum.pid)
- WebSocket connections are unstable but tab registration works
- Screenshot system exists but file saving is broken
- Need to create users section with working drawer buttons

## Key Ideas to Preserve

### Users Section Requirements
- Show connected users with avatars/status
- Working drawer buttons that actually respond to clicks
- Follow existing UI patterns (glass morphism, cyan gradients)
- Position: fixed, right side, don't cover existing sidebar
- Small contextual pane, not massive overlay

### Technical Approach
1. Use existing command structure in `src/commands/` 
2. Follow modular OOP patterns, not monolithic code
3. Test each small change before moving on
4. Commit working states with git

### Screenshot System Issues to Fix Later
- WebSocket handler needs `screenshot_captured` message type
- ScreenshotService.saveBrowserScreenshot() needs to actually save files to `.continuum/screenshots/`
- Browser-side needs working WebSocket connection to send data back

### Design Patterns Established
- Glass morphism: `rgba(0, 255, 136, 0.15)` with `backdrop-filter: blur(15px)`
- Angular video game aesthetics with clip-path polygons
- Smooth animations: `cubic-bezier(0.4, 0, 0.2, 1)`
- Hover effects with scale transforms and glow

## Next Steps
1. Create basic users section with DOM manipulation
2. Test it works
3. Commit it
4. Then iterate with proper testing