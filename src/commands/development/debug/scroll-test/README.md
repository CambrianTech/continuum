# debug/scroll-test - Animated Scroll Testing Command

Essential debug command for testing scroll behaviors, intersection observers, and chat positioning.

## Usage

```bash
# Basic animated scroll to top (smooth)
./jtag debug/scroll-test --target=top

# Instant scroll to bottom
./jtag debug/scroll-test --target=bottom --behavior=instant

# Scroll to specific position with metrics
./jtag debug/scroll-test --target=position --position=500 --captureMetrics=true

# Custom selector with wait time
./jtag debug/scroll-test --target=top --selector="main-widget .content" --waitTime=2000
```

## Parameters

- **target** (required): `'top'` | `'bottom'` | `'position'`
- **position**: Number (required if target=position)
- **behavior**: `'smooth'` | `'instant'` | `'auto'` (default: smooth)
- **selector**: CSS selector (default: chat-widget .chat-messages)
- **waitTime**: Milliseconds to wait after scroll (default: none)
- **captureMetrics**: Boolean - capture scroll metrics (default: false)

## Common Use Cases

### Testing Intersection Observer
```bash
# Smooth scroll to top to trigger intersection observer
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=1000

# Check if sentinel becomes visible and loadMore is triggered
./jtag debug/logs --filterPattern="Intersection|loadMore" --tailLines=10
```

### Chat Position Debugging
```bash
# Test scroll to bottom (newest messages)
./jtag debug/scroll-test --target=bottom --behavior=instant

# Test scroll restoration position
./jtag debug/scroll-test --target=position --position=300 --waitTime=500
```

### Scroll Behavior Analysis
```bash
# Capture full metrics during smooth scroll
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=2000

# Compare with instant scroll
./jtag debug/scroll-test --target=top --behavior=instant --captureMetrics=true
```

## Metrics Captured

When `captureMetrics=true`:
- **scrollHeight**: Total scrollable content height
- **clientHeight**: Visible container height
- **messagesCount**: Number of messages (for chat debugging)
- **sentinelVisible**: Whether intersection observer sentinel is visible

## Integration with Other Debug Commands

```bash
# Full debugging workflow
./jtag debug/scroll-test --target=top --captureMetrics=true
./jtag debug/logs --filterPattern="EntityScroller" --tailLines=20
./jtag interface/screenshot --querySelector="chat-widget" --filename="after-scroll.png"
```

## Architecture

- **Shared**: Clean parameter types and presets
- **Browser**: Shadow DOM traversal and metrics capture
- **Server**: Parameter validation and routing
- **Environment Agnostic**: Works with any scrollable container