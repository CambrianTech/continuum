# Screenshot Command

## Definition
- **Name**: screenshot
- **Description**: Capture browser screenshot with advanced targeting
- **Category**: Core
- **Icon**: 📸
- **Status**: 🟡 TESTING (README-driven migration in progress)
- **Parameters**: `[selector] [filename] [subdirectory] [scale]`

## Overview
The Screenshot command provides powerful browser screenshot capabilities with CSS selector targeting, custom file naming, and organized storage. Supports both full-page and element-specific captures.

## Parameters
- `selector`: CSS selector for element to capture (default: 'body' for full page)
- `filename`: Custom filename for screenshot (optional)
- `subdirectory`: Subdirectory within screenshots workspace (optional)
- `scale`: Scale factor for capture (default: 1.0)
- `format`: Image format (png, jpg, webp - default: png)
- `quality`: Image quality for lossy formats (0-100, default: 90)

## Usage Examples
```bash
# Full page screenshot
python3 ai-portal.py --cmd screenshot

# Capture specific element
python3 ai-portal.py --cmd screenshot --params '{"selector": "#main-content"}'

# Custom filename and directory
python3 ai-portal.py --cmd screenshot --params '{"filename": "my-capture", "subdirectory": "tests"}'

# High resolution capture
python3 ai-portal.py --cmd screenshot --params '{"scale": 2.0, "format": "png"}'

# Target specific widget
python3 ai-portal.py --cmd screenshot --params '{"selector": ".user-widget", "filename": "user-widget-test"}'
```

## Package Rules
```json
{
  "timeouts": {"client": 30.0, "server": 15.0},
  "retries": {"client": 2, "server": 1},
  "resources": {
    "client": ["display_access", "file_system"],
    "server": ["screenshot_api", "file_storage", "browser_connection"]
  },
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["creates_files", "system_capture"]
}
```

## Architecture
- **Dual-side execution**: Client handles display, server handles browser integration
- **CSS targeting**: Precise element capture using selectors
- **Organized storage**: Screenshots saved to `.continuum/screenshots/` workspace
- **Format support**: Multiple image formats with quality control
- **Browser integration**: Works with active browser connections

## Advanced Features
- **Element highlighting**: Can highlight captured elements
- **Batch capture**: Multiple screenshots in sequence
- **Responsive testing**: Different viewport sizes
- **Performance monitoring**: Capture timing and file size metrics

## Browser Integration
The screenshot command integrates with Continuum's browser automation:
- Requires active browser connection via WebSocket
- Supports multiple tabs and windows
- Can capture iframes and shadow DOM elements
- Handles dynamic content and animations

## File Organization
```
.continuum/screenshots/
├── screenshot-YYYYMMDD-HHMMSS.png     # Default naming
├── subdirectory/                       # Custom subdirectories
│   └── custom-filename.png            # Custom filenames
└── tests/                             # Organized by purpose
    ├── widget-tests/
    └── full-page-captures/
```

## Performance Notes
- **Server timeout**: 15s for capture and processing
- **Client timeout**: 30s including network transfer
- **Concurrent captures**: Multiple screenshots can run in parallel
- **Memory management**: Large screenshots are streamed, not buffered

## TODO: Improvements Needed
- ✅ ~~Implement README-driven getDefinition() method~~ (COMPLETED)
- TODO: Add package.json with dual-side timeout definitions
- TODO: Verify CSS selector validation 
- TODO: Test concurrent screenshot handling
- TODO: Add error handling for invalid selectors
- TODO: Implement quality settings for different formats

## WARNING: Status
- ⚠️ **NOT FULLY TESTED**: Command migrated to README-driven but needs verification
- ⚠️ **BREAKING CHANGES POSSIBLE**: Parameter format may have changed
- ⚠️ **MISSING PACKAGE RULES**: No package.json with timeout definitions yet