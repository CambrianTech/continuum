# Continuum Emotional Feedback System

## Vision: HAL 9000 Meets Tron

The Continuum widget is the **shared emotional interface** between humans and AIs - a universal visual language for AI→human feedback. Inspired by HAL 9000's iconic red eye and Tron's dynamic light cycles, it provides real-time visual feedback for AI actions, system status, and collaborative browsing.

**Core Concept**: "The life of the AI" - a pulsating dot that expresses emotion, indicates system state, and transforms into dynamic visual feedback when AIs take actions.

**Port 9000**: Named after HAL 9000, hence the system runs on port 9000 (currently 9003, will settle on 9000).

---

## Architecture Overview

### Three-Layer Control System

```
┌─────────────────────────────────────────────────────┐
│  System Status (Default)                            │
│  • Pulsating dot (emotion/health)                   │
│  • Connection state                                 │
│  • Background activity indicators                   │
└─────────────────────────────────────────────────────┘
                    ↑
┌─────────────────────────────────────────────────────┐
│  Commands (Temporary Control)                       │
│  • Anyone can call continuum/set                    │
│  • Display custom emoji/color/message               │
│  • Auto-revert after duration                       │
└─────────────────────────────────────────────────────┘
                    ↑
┌─────────────────────────────────────────────────────┐
│  Events (Real-time Feedback)                        │
│  • Screenshot animations                            │
│  • Tool execution indicators                        │
│  • AI cursor/pointer                                │
└─────────────────────────────────────────────────────┘
```

**Priority**: Events > Commands > System Status

---

## Features

### 1. Universal Control via Commands

**Any actor** (human via CLI, PersonaUser, external AI, tests) can temporarily control the Continuum widget:

```bash
# Human via CLI
./jtag continuum/set --emoji="🔍" --color="blue" --message="Searching codebase"

# PersonaUser in code
await Commands.execute('continuum/set', {
  emoji: '🤖',
  color: 'green',
  message: 'Processing your request',
  duration: 5000  // Auto-revert after 5s
});

# Clear and return to system status
./jtag continuum/set --clear=true
```

**Parameters**:
- `emoji?: string` - Display emoji in/near the dot
- `color?: string` - Dot color (CSS color value)
- `message?: string` - Text displayed under the dot
- `duration?: number` - Auto-revert to system status after N milliseconds
- `clear?: boolean` - Immediately return to system status

### 2. Tron Light Cycle Animations

**Screenshot command** automatically triggers fast-moving rectangular animation around the captured area:

```bash
# Screenshot with animation (default ON)
./jtag interface/screenshot --querySelector="chat-widget"
# → Draws Tron-style light cycle trail around crop rectangle

# Screenshot without animation
./jtag interface/screenshot --querySelector="chat-widget" --noAnimation=true
```

**How it works**:
1. Screenshot command captures coordinates (x, y, width, height)
2. Emits `screenshot:captured` event with coordinates
3. ContinuumWidget draws fast-moving perimeter animation
4. Animation completes in ~500ms and auto-removes

**Visual Effect**:
- Cyan/blue glowing border
- Draws from top-left clockwise (like light cycle trail)
- Box shadow for neon glow effect
- High z-index (999999) to appear above all content

### 3. AI Cursor / Shared Pointer

**The Continuum dot doubles as the AI's cursor** for:
- Pointing to UI elements AIs are analyzing
- Indicating what AIs are looking at during collaboration
- Shared browsing (human + AI browse web together)

**Usage**:
```typescript
// AI wants to point at an element
await Commands.execute('continuum/point', {
  querySelector: '.chat-message:last-child',
  duration: 3000,  // Point for 3 seconds
  message: 'This message looks suspicious'
});

// AI wants to indicate a screen region
await Commands.execute('continuum/point', {
  coordinates: { x: 100, y: 200, width: 300, height: 150 },
  duration: 2000,
  color: 'yellow'
});
```

**Movement**:
- **Bezier curve animations** for natural, organic movement
- Ease-in-ease-out timing functions
- Smooth transitions between points
- Never teleports - always animates

**Bezier Animation Example**:
```typescript
// Move cursor with natural bezier curve
function animateCursorTo(targetX: number, targetY: number, duration: number = 800) {
  const startX = currentX;
  const startY = currentY;
  const startTime = Date.now();

  function animate() {
    const elapsed = Date.now() - startTime;
    const progress = Math.min(elapsed / duration, 1);

    // Cubic bezier easing (ease-in-out)
    const eased = progress < 0.5
      ? 4 * progress * progress * progress
      : 1 - Math.pow(-2 * progress + 2, 3) / 2;

    currentX = startX + (targetX - startX) * eased;
    currentY = startY + (targetY - startY) * eased;

    updateCursorPosition(currentX, currentY);

    if (progress < 1) {
      requestAnimationFrame(animate);
    }
  }

  requestAnimationFrame(animate);
}
```

### 4. Dynamic Favicon Updates

**Browser tab favicon** reflects Continuum state in real-time:

```typescript
// Update favicon to match Continuum emotion/status
Events.subscribe('continuum:status', (data) => {
  updateFavicon({
    color: data.color,
    emotion: data.emoji,
    status: data.systemStatus
  });
});
```

**Favicon States**:
- 🟢 Green pulsing = Healthy, idle
- 🔵 Blue animated = Processing/thinking
- 🟡 Yellow steady = Warning/attention needed
- 🔴 Red flashing = Error/critical
- ⚡ Lightning bolt = Tool execution in progress

**Implementation**: Canvas-based dynamic favicon generation (like notification badges).

### 5. Shared Web Browsing

**Problem**: CORS restrictions prevent AIs from directly accessing arbitrary web content in user's browser.

**Solution**: Custom `web-browser-widget` that:
1. Loads web pages in isolated iframe with proxy
2. Continuum dot becomes AI's cursor **inside the web content**
3. AI can point, highlight, scroll, and indicate areas of interest
4. Human and AI browse together in real-time

```bash
# Human opens web browser widget
./jtag browser/open --url="https://docs.anthropic.com"

# AI indicates interesting section
await Commands.execute('continuum/point', {
  querySelector: '#rate-limits',
  message: 'Found the rate limit documentation',
  highlight: true  // Adds temporary yellow highlight
});

# AI scrolls to specific section
await Commands.execute('browser/scroll', {
  querySelector: '#authentication',
  smooth: true
});
```

**CORS Workaround**:
- Server-side proxy for fetching web content
- Content sanitization and injection into iframe
- AI pointer overlay rendered above iframe
- Click/scroll events synthesized via JavaScript injection

---

## Event System

### Events Emitted

```typescript
// Screenshot captured (triggers animation)
Events.emit('screenshot:captured', {
  coordinates: { x: 100, y: 200, width: 300, height: 150 },
  querySelector: 'chat-widget',
  timestamp: Date.now(),
  animate: true  // Default true unless --noAnimation flag
});

// Continuum status change
Events.emit('continuum:status', {
  emoji: '🤖',
  color: 'green',
  message: 'Processing',
  source: 'persona-ai',  // or 'system', 'command', 'event'
  priority: 'high',  // 'low' | 'medium' | 'high' | 'critical'
  duration?: 5000  // Auto-revert timeout
});

// AI pointer movement
Events.emit('continuum:point', {
  target: { x: 150, y: 250 },  // or querySelector
  message?: 'Looking at this',
  color?: 'cyan',
  duration: 3000,
  animate: true  // Use bezier curve
});

// System health change
Events.emit('system:health', {
  status: 'healthy' | 'degraded' | 'error',
  message: string,
  metrics: { cpu: number, memory: number, ... }
});
```

### Events Subscribed

```typescript
// ContinuumWidget listens for these events
class ContinuumWidget {
  connectedCallback() {
    // Screenshot animations
    Events.subscribe('screenshot:captured', (data) => {
      if (data.animate) {
        this.animateTronRectangle(data.coordinates);
      }
    });

    // Status updates
    Events.subscribe('continuum:status', (data) => {
      this.updateStatus(data);
      if (data.duration) {
        setTimeout(() => this.revertToSystemStatus(), data.duration);
      }
    });

    // Pointer/cursor movement
    Events.subscribe('continuum:point', (data) => {
      this.movePointerTo(data.target, data.animate);
    });

    // System health
    Events.subscribe('system:health', (data) => {
      this.updateHealthIndicator(data);
    });
  }
}
```

---

## Commands API

### `continuum/set` - Temporary Status Control

**Purpose**: Anyone can temporarily control the Continuum widget to display custom status.

**Parameters**:
```typescript
interface ContinuumSetParams extends CommandParams {
  emoji?: string;      // Emoji to display
  color?: string;      // CSS color value
  message?: string;    // Text under the dot
  duration?: number;   // Auto-revert after N ms (default: 5000)
  clear?: boolean;     // Immediately return to system status
}
```

**Examples**:
```bash
./jtag continuum/set --emoji="🔍" --message="Analyzing code"
./jtag continuum/set --color="red" --message="Error detected" --duration=10000
./jtag continuum/set --clear=true
```

### `continuum/point` - AI Cursor Control

**Purpose**: Move the Continuum dot to point at specific UI elements or coordinates.

**Parameters**:
```typescript
interface ContinuumPointParams extends CommandParams {
  querySelector?: string;           // CSS selector to point at
  coordinates?: Rectangle;          // Or explicit x,y coordinates
  message?: string;                 // Tooltip/message
  color?: string;                   // Pointer color
  duration?: number;                // How long to point (default: 3000)
  animate?: boolean;                // Use bezier curve (default: true)
  highlight?: boolean;              // Add temporary highlight to target
}
```

**Examples**:
```bash
./jtag continuum/point --querySelector=".error-message" --message="Found the issue"
./jtag continuum/point --coordinates='{"x":100,"y":200}' --color="yellow"
```

### `screenshot` - Enhanced with Animation

**New Parameter**:
```typescript
interface ScreenshotParams extends CommandParams {
  // ... existing params ...
  noAnimation?: boolean;  // Disable Tron rectangle animation (default: false)
}
```

**Examples**:
```bash
./jtag interface/screenshot --querySelector="body"  # Animates by default
./jtag interface/screenshot --querySelector="chat-widget" --noAnimation=true
```

---

## Implementation Details

### Tron Animation (CSS + Canvas Hybrid)

**Option 1: Pure CSS** (Simple, performant)
```typescript
animateTronRectangle(coords: Rectangle): void {
  const overlay = document.createElement('div');
  overlay.className = 'tron-animation';
  overlay.style.cssText = `
    position: absolute;
    left: ${coords.x}px;
    top: ${coords.y}px;
    width: ${coords.width}px;
    height: ${coords.height}px;
    border: 2px solid cyan;
    box-shadow: 0 0 10px cyan, inset 0 0 10px cyan;
    pointer-events: none;
    z-index: 999999;
    animation: tron-draw 0.5s ease-out forwards;
  `;

  document.body.appendChild(overlay);

  setTimeout(() => overlay.remove(), 600);
}
```

**CSS Animation**:
```css
@keyframes tron-draw {
  0% {
    clip-path: polygon(0 0, 0 0, 0 0, 0 0);
  }
  25% {
    clip-path: polygon(0 0, 100% 0, 100% 0, 0 0);
  }
  50% {
    clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
  }
  75% {
    clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
  }
  100% {
    clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
    opacity: 0;
  }
}
```

**Option 2: Canvas** (Complex, allows particle trails)
```typescript
animateTronRectangleCanvas(coords: Rectangle): void {
  const canvas = document.createElement('canvas');
  canvas.width = window.innerWidth;
  canvas.height = window.innerHeight;
  canvas.style.cssText = `
    position: fixed;
    top: 0;
    left: 0;
    pointer-events: none;
    z-index: 999999;
  `;
  document.body.appendChild(canvas);

  const ctx = canvas.getContext('2d')!;
  const duration = 500;
  const startTime = Date.now();

  function animate() {
    const elapsed = Date.now() - startTime;
    const progress = Math.min(elapsed / duration, 1);

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Draw glowing rectangle with particle trail
    ctx.strokeStyle = `rgba(0, 255, 255, ${1 - progress})`;
    ctx.lineWidth = 2;
    ctx.shadowBlur = 10;
    ctx.shadowColor = 'cyan';

    // Calculate current perimeter drawing position
    const perimeterLength = 2 * (coords.width + coords.height);
    const currentLength = perimeterLength * progress;

    // Draw path up to current position
    // ... (complex path calculation)

    if (progress < 1) {
      requestAnimationFrame(animate);
    } else {
      canvas.remove();
    }
  }

  requestAnimationFrame(animate);
}
```

### Bezier Cursor Movement

```typescript
class ContinuumCursor {
  private currentX: number = 0;
  private currentY: number = 0;
  private element: HTMLElement;

  constructor(element: HTMLElement) {
    this.element = element;
  }

  /**
   * Move cursor with cubic bezier easing
   * Creates natural, organic movement
   */
  moveTo(targetX: number, targetY: number, duration: number = 800): Promise<void> {
    return new Promise((resolve) => {
      const startX = this.currentX;
      const startY = this.currentY;
      const startTime = Date.now();

      const animate = () => {
        const elapsed = Date.now() - startTime;
        const progress = Math.min(elapsed / duration, 1);

        // Cubic bezier easing (ease-in-out)
        // Similar to CSS cubic-bezier(0.42, 0, 0.58, 1)
        const eased = progress < 0.5
          ? 4 * progress * progress * progress
          : 1 - Math.pow(-2 * progress + 2, 3) / 2;

        this.currentX = startX + (targetX - startX) * eased;
        this.currentY = startY + (targetY - startY) * eased;

        this.element.style.transform = `translate(${this.currentX}px, ${this.currentY}px)`;

        if (progress < 1) {
          requestAnimationFrame(animate);
        } else {
          resolve();
        }
      };

      requestAnimationFrame(animate);
    });
  }

  /**
   * Move to element with optional offset
   */
  async moveToElement(element: Element, offset: {x: number, y: number} = {x: 0, y: 0}): Promise<void> {
    const rect = element.getBoundingClientRect();
    const targetX = rect.left + rect.width / 2 + offset.x;
    const targetY = rect.top + rect.height / 2 + offset.y;
    return this.moveTo(targetX, targetY);
  }
}
```

### Dynamic Favicon Generation

```typescript
class FaviconManager {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private link: HTMLLinkElement;

  constructor() {
    this.canvas = document.createElement('canvas');
    this.canvas.width = 32;
    this.canvas.height = 32;
    this.ctx = this.canvas.getContext('2d')!;

    this.link = document.querySelector('link[rel="icon"]') || document.createElement('link');
    this.link.rel = 'icon';
    document.head.appendChild(this.link);
  }

  /**
   * Update favicon to reflect current status
   */
  update(status: {color: string, emotion?: string, pulse?: boolean}): void {
    const ctx = this.ctx;
    ctx.clearRect(0, 0, 32, 32);

    // Draw pulsating dot
    const pulse = status.pulse ? Math.sin(Date.now() / 200) * 0.2 + 0.8 : 1;
    const radius = 12 * pulse;

    ctx.fillStyle = status.color;
    ctx.beginPath();
    ctx.arc(16, 16, radius, 0, Math.PI * 2);
    ctx.fill();

    // Add glow
    ctx.shadowBlur = 5;
    ctx.shadowColor = status.color;
    ctx.fill();

    // Add emoji if provided (small, top-right)
    if (status.emotion) {
      ctx.shadowBlur = 0;
      ctx.font = '12px Arial';
      ctx.fillText(status.emotion, 20, 8);
    }

    // Convert to data URL and update favicon
    this.link.href = this.canvas.toDataURL('image/png');

    // If pulsing, schedule next frame
    if (status.pulse) {
      requestAnimationFrame(() => this.update(status));
    }
  }
}
```

---

## Use Cases

### 1. AI Taking Screenshot

```typescript
// PersonaUser takes screenshot
const result = await Commands.execute('screenshot', {
  querySelector: 'chat-widget',
  context: this.context,
  sessionId: this.sessionId
});

// Screenshot command automatically emits event:
Events.emit('screenshot:captured', {
  coordinates: result.coordinates,
  querySelector: 'chat-widget',
  animate: true  // Default
});

// ContinuumWidget animates Tron rectangle
// Human SEES the light cycle animation around chat widget
// Immediate visual feedback: "AI is looking at the chat"
```

### 2. AI Indicating Status

```typescript
// AI starts analyzing code
await Commands.execute('continuum/set', {
  emoji: '🔍',
  color: 'blue',
  message: 'Analyzing codebase',
  duration: 60000  // 1 minute
});

// Do analysis work...

// AI finds issue
await Commands.execute('continuum/set', {
  emoji: '⚠️',
  color: 'yellow',
  message: 'Found potential issue',
  duration: 5000
});

// AI points to problematic code
await Commands.execute('continuum/point', {
  querySelector: '.error-line',
  message: 'This function has a memory leak',
  highlight: true
});
```

### 3. Collaborative Web Browsing

```typescript
// Human opens documentation
await Commands.execute('browser/open', {
  url: 'https://docs.anthropic.com/rate-limits'
});

// AI reads page content (via proxy)
const content = await Commands.execute('browser/content', {
  querySelector: 'body'
});

// AI finds relevant section
await Commands.execute('continuum/point', {
  querySelector: '#rate-limits-table',
  message: 'Here are the rate limits',
  duration: 5000
});

// AI scrolls to show example
await Commands.execute('browser/scroll', {
  querySelector: '#example-code',
  smooth: true
});

// Human sees cursor move, page scroll, AI guiding them
```

### 4. System Health Monitoring

```typescript
// System detects high memory usage
Events.emit('system:health', {
  status: 'degraded',
  message: 'High memory usage detected',
  metrics: { memory: 85, cpu: 45 }
});

// ContinuumWidget updates:
// - Dot color changes to yellow
// - Pulsation rate increases (anxiety)
// - Favicon shows yellow dot
// - Tooltip shows "High memory usage"

// Human glances at browser tab → sees yellow favicon → knows something needs attention
```

---

## Priority and Conflict Resolution

**When multiple sources try to control Continuum simultaneously:**

1. **Critical system errors** always override (red dot, error message)
2. **Events** (screenshot, tool execution) temporarily override commands
3. **Commands** override default system status
4. **System status** is the fallback when nothing else active

**Duration-based auto-revert**:
- Commands with `duration` parameter auto-revert after timeout
- Events (like screenshots) are transient (~500ms)
- System status is permanent until overridden

**Example conflict resolution**:
```typescript
// System status: Green pulsing (healthy)
// ↓
// Command: Blue "Processing" (overrides system status)
// ↓
// Event: Screenshot animation (temporarily overrides command)
// ↓ (animation ends after 500ms)
// Command: Blue "Processing" (resumes)
// ↓ (duration expires after 5000ms)
// System status: Green pulsing (reverts to default)
```

---

## Future Enhancements

### Phase 1: Core Implementation (NEXT)
- ✅ `continuum/set` command
- ✅ `continuum/point` command
- ✅ Screenshot animation (Tron rectangle)
- ✅ Event system wiring
- ✅ Basic bezier cursor movement

### Phase 2: Advanced Animations
- Particle trail effects for cursor
- Multiple cursors (multiple AIs collaborating)
- Gesture recognition (AI "drawing" on screen)
- Smooth scroll following cursor

### Phase 3: Collaborative Browsing
- Web browser widget with CORS proxy
- AI cursor inside iframe content
- Content extraction and analysis
- Synchronized scrolling/navigation

### Phase 4: Emotional Intelligence
- Mood detection from AI messages
- Pulsation rate matches AI "anxiety" level
- Color gradients for mixed emotions
- Sound effects for critical events

### Phase 5: Multi-User
- Multiple human users see same Continuum state
- Each user has own cursor (different colors)
- Collaborative debugging sessions
- Shared attention indicators

---

## Testing Strategy

```bash
# Manual testing
npm start
./jtag continuum/set --emoji="🔍" --message="Testing"
./jtag interface/screenshot --querySelector="body"
./jtag continuum/point --querySelector="chat-widget"

# Visual verification
./jtag interface/screenshot --querySelector="continuum-widget" --output="/tmp/continuum-test.png"

# Event testing
./jtag debug/events --filter="continuum:*"

# Integration test
npx vitest tests/integration/continuum-feedback.test.ts
```

---

## Why This Matters

**The Continuum emotional feedback system is crucial for AI-human collaboration because:**

1. **Immediate Visual Feedback**: Humans instantly SEE what AIs are doing (no need to read logs)
2. **Shared Attention**: Cursor shows where AI is "looking" (builds trust and understanding)
3. **Emotional Language**: Colors, pulsation, emojis communicate AI state intuitively
4. **Universal Control**: Any actor can participate (democratizes system communication)
5. **Non-Intrusive**: Peripheral awareness (favicon, dot) doesn't interrupt workflow
6. **Collaborative Browsing**: Human and AI explore content together (pair programming for research)

**This transforms the relationship from:**
- ❌ Human types command → AI responds → Human reads text output
- ✅ Human and AI share visual space → AI points, shows, guides → Human follows naturally

**It's the missing link between**:
- Text-based AI communication (isolated, abstract)
- Embodied AI collaboration (shared visual context, intuitive)

---

## Technical Notes

**Performance Considerations**:
- Animations use `requestAnimationFrame` (60fps, hardware-accelerated)
- Canvas rendering only when needed (screenshot animations)
- CSS transforms for cursor movement (GPU-accelerated)
- Event debouncing for rapid status updates
- Automatic cleanup of DOM elements after animations

**Browser Compatibility**:
- Chrome/Edge: Full support
- Firefox: Full support
- Safari: Full support (may need `-webkit-` prefixes)
- Mobile: Limited (no hover, different coordinate system)

**Security**:
- CORS proxy validates and sanitizes web content
- No direct execution of injected scripts
- Cursor coordinates validated and bounded
- Rate limiting on continuum/set commands (prevent spam)

**Accessibility**:
- ARIA labels for screen readers
- Keyboard navigation for cursor control
- High contrast mode support
- Optional audio cues for critical events

---

## References

- **HAL 9000**: *2001: A Space Odyssey* - iconic AI with visual presence
- **Tron**: Light cycle trails and neon aesthetics
- **Port 9000**: System runs on port 9000 as homage to HAL 9000
- **Bezier Curves**: Natural, organic motion (similar to Apple's spring animations)
- **Events System**: `docs/UNIVERSAL-PRIMITIVES.md`
- **Commands System**: `docs/UNIVERSAL-PRIMITIVES.md`
- **Screenshot Command**: `commands/screenshot/`

---

**Document Status**: Design specification (implementation pending)
**Last Updated**: 2025-11-26
**Author**: Joel + Claude Code
**Version**: 1.0.0
