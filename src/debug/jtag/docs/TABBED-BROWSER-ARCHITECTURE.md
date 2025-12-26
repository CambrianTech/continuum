# Tabbed Browser Architecture

## Problem

The current `navigate` command uses `window.location.href` which navigates **the entire Continuum widget tab away**, breaking the UI. An AI running `./jtag navigate --url="http://localhost:9002"` destroys the widget interface.

**Current broken behavior:**
```typescript
// commands/navigate/browser/NavigateBrowserCommand.ts:40
window.location.href = params.url;  // ❌ Navigates away from Continuum UI
```

## Solution: Browser Widget as Center Content Panel

Create a **browser-widget** that displays in the center content area (like chat rooms, settings, etc.), allowing humans and AIs to collaboratively browse, annotate, and screenshot web pages together.

### Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│ Continuum Widget (localhost:9003)                        │
│                                                           │
│  ┌─────────────────────────────────────────────────┐    │
│  │ Main Widget                                      │    │
│  │                                                   │    │
│  │  [Tabs: General | Academy | Browser]            │    │
│  │                                                   │    │
│  │  CENTER CONTENT (switchable):                    │    │
│  │  ┌───────────────────────────────────────────┐  │    │
│  │  │ <chat-widget> (room content)              │  │    │
│  │  │ OR                                        │  │    │
│  │  │ <browser-widget>                          │  │    │
│  │  │   ┌─────┬─────┬─────┐                    │  │    │
│  │  │   │Tab 1│Tab 2│Tab 3│ + [New]            │  │    │
│  │  │   └─────┴─────┴─────┘                    │  │    │
│  │  │   ┌───────────────────────────────────┐  │  │    │
│  │  │   │ <iframe>                          │  │  │    │
│  │  │   │   src="/proxy?url=example.com"    │  │  │    │
│  │  │   │                                   │  │  │    │
│  │  │   │ [Proxied web content]             │  │  │    │
│  │  │   │                                   │  │  │    │
│  │  │   │ <canvas> (cursor/annotation)      │  │  │    │
│  │  │   └───────────────────────────────────┘  │  │    │
│  │  └───────────────────────────────────────────┘  │    │
│  │                                                   │    │
│  │  [Sidebar: Rooms | Users]                        │    │
│  └─────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
                         ▲
                         │ WebSocket (Commands)
                         ▼
┌──────────────────────────────────────────────────────────┐
│ Backend Server (localhost:9002)                          │
│  ┌─────────────────────┐  ┌────────────────────────┐    │
│  │ Browser Tabs Daemon │  │ Web Proxy Middleware   │    │
│  │ - Tab state         │  │ - Fetch external pages │    │
│  │ - Cursor positions  │  │ - CORS bypass          │    │
│  │ - Annotations       │  │ - Sanitize content     │    │
│  └─────────────────────┘  └────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
                         │
                         ▼
                External Web (example.com, etc.)
```

### Key Insight: Browser Widget = Activity Type

The browser widget is **NOT embedded within chat** - it's a **separate activity** that occupies the center content area, just like:
- Chat rooms (`/chat/general`)
- Settings panel (`/settings`)
- Code editor (`/editor`)
- Browser (`/browser`)

## Components

### 1. Browser Tabs Widget (`widgets/browser-tabs/`)

**Responsibilities:**
- Display tabs UI with add/close functionality
- Render iframe with proxied content
- Overlay canvas for cursor/annotations
- Emit events for tab changes, navigation, etc.

**Key Files:**
```
widgets/browser-tabs/
├── BrowserTabsWidget.ts          # Web component
├── BrowserTabsStyles.css         # Tab bar styling
├── TabManager.ts                 # Tab state management
└── AnnotationCanvas.ts           # Cursor/drawing overlay
```

**Usage:**
```typescript
// Automatically included in main-widget
<browser-tabs-widget></browser-tabs-widget>
```

### 2. Backend Proxy Middleware (`server/middleware/web-proxy.ts`)

**Responsibilities:**
- Fetch external web pages
- Bypass CORS restrictions
- Sanitize HTML (prevent XSS)
- Inject annotation scripts
- Cache responses

**Endpoint:**
```
GET /proxy?url=<encoded-url>
Returns: Proxied HTML content with injected scripts
```

**Security:**
```typescript
// Only allow http/https URLs
// Sanitize HTML to prevent script injection
// Inject annotation bridge scripts
// Set CSP headers appropriately
```

### 3. Browser Tabs Daemon (`daemons/browser-tabs-daemon/`)

**Responsibilities:**
- Track tab state across all connected clients
- Sync cursor positions between users
- Store annotations/drawings
- Broadcast tab events to all viewers

**Commands:**
```bash
./jtag tabs/create --url="https://example.com"
./jtag tabs/close --tabId="abc123"
./jtag tabs/list
./jtag tabs/annotate --tabId="abc123" --annotation={...}
./jtag tabs/cursor --tabId="abc123" --x=100 --y=200 --userId="joel"
```

**Types:**
```typescript
interface BrowserTab {
  id: UUID;
  url: string;
  title: string;
  createdAt: Date;
  createdBy: UUID;  // userId
  viewers: UUID[];  // Current viewers
  annotations: Annotation[];
  cursors: Map<UUID, CursorPosition>;
}

interface Annotation {
  id: UUID;
  type: 'cursor' | 'highlight' | 'arrow' | 'text';
  userId: UUID;
  x: number;
  y: number;
  data: any;  // Type-specific data
}

interface CursorPosition {
  userId: UUID;
  x: number;
  y: number;
  timestamp: Date;
}
```

### 4. Updated Navigate Command

**Change:**
```typescript
// OLD: Navigate entire window away
window.location.href = params.url;  // ❌

// NEW: Switch to browser activity and open URL
// Step 1: Switch center content to browser widget
await MainWidget.switchContent('/browser');  // Like switching to /chat/general

// Step 2: Create tab in browser widget
await Commands.execute('tabs/create', {
  url: params.url,
  focus: true  // Switch to this tab
});  // ✅
```

**Content Switching Pattern (MainWidget.ts:105-111):**
```typescript
private updateContentView(pathType: string): void {
  const contentView = this.shadowRoot?.querySelector('.content-view');
  if (!contentView) return;

  // Switch center content based on path type
  switch (pathType) {
    case 'chat':
      contentView.innerHTML = '<chat-widget></chat-widget>';
      break;
    case 'browser':
      contentView.innerHTML = '<browser-widget></browser-widget>';  // NEW
      break;
    case 'settings':
      contentView.innerHTML = '<settings-widget></settings-widget>';
      break;
  }
}
```

**Backwards Compatibility:**
```typescript
// Add flag to choose behavior
interface NavigateParams extends CommandParams {
  url: string;
  useEmbeddedBrowser?: boolean;  // Default: true
  waitForSelector?: string;
  timeout?: number;
}
```

## Implementation Phases

### Phase 1: Basic Tabbed Browser (MVP)
**Goal:** Display external web pages in tabs without breaking Continuum UI

- [ ] Create `BrowserTabsWidget` web component
- [ ] Implement tab bar UI (create/close/switch)
- [ ] Add iframe rendering with proxy endpoint
- [ ] Create `web-proxy` middleware with basic fetch
- [ ] Update `navigate` command to use tabs
- [ ] Test: `./jtag navigate --url="https://anthropic.com"` opens in tab

**Test:**
```bash
npm start
./jtag navigate --url="https://example.com"
# Should see tab open in browser tabs widget, not navigate away
./jtag interface/screenshot  # Should capture widget with embedded browser tab
```

### Phase 2: Backend Proxy & Security
**Goal:** Handle CORS, sanitize content, cache responses

- [ ] Implement CORS bypass in proxy
- [ ] Add HTML sanitization (DOMPurify or similar)
- [ ] Inject annotation bridge scripts
- [ ] Add response caching (15min TTL)
- [ ] Handle redirects properly
- [ ] Test with various sites (JS-heavy, CORS-protected, etc.)

### Phase 3: Browser Tabs Daemon
**Goal:** Multi-user tab state management

- [ ] Create `BrowserTabsDaemon` class
- [ ] Implement tab CRUD commands
- [ ] Add tab state persistence (database)
- [ ] Sync tab list across connected clients
- [ ] Events: `tabs:created`, `tabs:closed`, `tabs:switched`

### Phase 4: Collaborative Cursors
**Goal:** Show where other users are pointing

- [ ] Create `AnnotationCanvas` overlay component
- [ ] Implement cursor position broadcasting
- [ ] Render other users' cursors with labels
- [ ] Add cursor movement smoothing/interpolation
- [ ] Test with multiple users viewing same tab

### Phase 5: Annotations & Drawing
**Goal:** Highlight, draw arrows, add text to web pages

- [ ] Implement highlight tool
- [ ] Add arrow drawing
- [ ] Add text annotations
- [ ] Persist annotations to database
- [ ] Sync annotations across viewers
- [ ] Add clear/undo functionality

### Phase 6: Screenshot Integration
**Goal:** Screenshot command captures embedded browser content

- [ ] Update screenshot command to handle iframes
- [ ] Capture annotations/cursors in screenshots
- [ ] Add `--includeAnnotations` flag
- [ ] Test: AI takes screenshot of shared web page

## Key Design Decisions

### Why iframe + proxy?
- **Security**: Isolates external content from Continuum
- **CORS bypass**: Backend proxy fetches content
- **Sandboxing**: iframe provides CSP boundaries
- **Screenshot support**: Can capture iframe content

### Why not WebView or Electron?
- **Platform independence**: Works in any browser
- **No native dependencies**: Pure TypeScript/Node
- **Simpler deployment**: No Electron packaging

### Why separate daemon?
- **Multi-user sync**: Tab state shared across clients
- **Persistence**: Tabs survive browser refreshes
- **Event broadcasting**: All viewers get updates
- **Scalability**: Can handle many concurrent tabs

## Usage Examples

### Human-AI Collaborative Browsing

```bash
# Human opens a page
./jtag navigate --url="https://docs.anthropic.com/claude"

# AI analyzes what's visible
./jtag interface/screenshot --querySelector="browser-tabs-widget"
# AI: "I see the Claude API docs. Looking at the 'Messages' section..."

# AI points to specific section
./jtag tabs/cursor --x=250 --y=400 --userId="helper-ai-id"
./jtag tabs/annotate --type="arrow" --x=250 --y=400 --text="This parameter is important"

# Human sees AI's cursor and annotation in real-time
# Human can reply: "Good catch, let's try that parameter"

# AI can navigate to linked page
./jtag navigate --url="https://docs.anthropic.com/claude/messages"
# Opens in new tab, both human and AI see it
```

### Research Task

```bash
# Human: "Research competitor pricing"
./jtag collaboration/chat/send --room="general" --message="Can you research competitor pricing for me?"

# AI responds and navigates
./jtag navigate --url="https://competitor1.com/pricing"
./jtag interface/screenshot --querySelector="browser-tabs-widget"
# AI extracts pricing, opens new tab for next competitor

./jtag tabs/create --url="https://competitor2.com/pricing"
./jtag interface/screenshot
# AI compiles comparison, sends summary in chat

./jtag collaboration/chat/send --room="general" --message="Found 3 competitors: A ($10/mo), B ($15/mo), C ($20/mo). Screenshots in tabs."

# Human can review all tabs, see AI's annotations
```

## Future Enhancements

### Phase 7+: Advanced Features
- [ ] **Session recording**: Record browsing session as video
- [ ] **Diff view**: Compare two tabs side-by-side
- [ ] **Auto-scroll sync**: Scroll positions synced across viewers
- [ ] **Form filling**: AI can fill forms for user approval
- [ ] **Network inspection**: Show requests/responses (like DevTools)
- [ ] **Mobile viewport**: Test responsive designs
- [ ] **Bookmark system**: Save important tabs
- [ ] **Tab groups**: Organize tabs by topic/project

## Migration Strategy

### Rollout Plan

**Week 1: Phase 1 MVP**
- Build basic widget + proxy
- Update navigate command
- Deploy with feature flag

**Week 2: Test with AIs**
- Enable for helper-ai first
- Monitor chat for navigation attempts
- Fix issues, iterate on UX

**Week 3: Phases 2-3**
- Add security hardening
- Deploy daemon for multi-user
- Test with multiple personas

**Week 4: Phases 4-5**
- Add cursor tracking
- Deploy annotations
- Test collaborative features

**Week 5+: Polish & Phase 6**
- Screenshot integration
- Performance optimization
- Full production rollout

### Feature Flag

```typescript
// config/features.ts
export const FEATURES = {
  EMBEDDED_BROWSER: process.env.ENABLE_EMBEDDED_BROWSER === 'true'
};

// navigate command
if (FEATURES.EMBEDDED_BROWSER) {
  await Commands.execute('tabs/create', { url: params.url });
} else {
  window.location.href = params.url;  // Old behavior
}
```

## Success Metrics

**MVP Success Criteria:**
- ✅ Navigate command doesn't break Continuum UI
- ✅ Can open multiple tabs without issues
- ✅ Screenshots capture embedded browser content
- ✅ No XSS vulnerabilities in proxy
- ✅ Works with major websites (anthropic.com, github.com, etc.)

**Collaborative Success Criteria:**
- ✅ Multiple users see same tab content
- ✅ Cursor positions sync in <100ms
- ✅ Annotations persist across page refreshes
- ✅ AIs can effectively point to page elements
- ✅ Humans can follow AI's browsing flow

## Security Considerations

### Proxy Security
```typescript
// Whitelist allowed protocols
const ALLOWED_PROTOCOLS = ['http:', 'https:'];

// Sanitize HTML before serving
import DOMPurify from 'dompurify';
const clean = DOMPurify.sanitize(html);

// CSP headers
response.setHeader('Content-Security-Policy',
  "default-src 'self'; script-src 'unsafe-inline' 'unsafe-eval'; img-src *;");
```

### iframe Sandbox
```html
<iframe
  sandbox="allow-scripts allow-same-origin allow-forms"
  src="/proxy?url=..."
></iframe>
```

### Rate Limiting
```typescript
// Limit proxy requests per user
const RATE_LIMIT = {
  windowMs: 60000,  // 1 minute
  max: 30  // 30 requests per minute
};
```

---

## Summary

The tabbed browser architecture solves the **navigate command breaking Continuum UI** by embedding a multi-tab browser inside the widget. This enables:

1. **Safe navigation**: Never leaves Continuum
2. **Collaborative browsing**: Humans and AIs view together
3. **Screenshot support**: Capture web content in context
4. **Annotations**: Point to specific page elements
5. **Multi-tab**: Research multiple pages simultaneously

**Next Step**: Implement Phase 1 MVP - basic tabs widget with proxy.
