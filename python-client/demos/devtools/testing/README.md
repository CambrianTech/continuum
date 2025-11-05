# DevTools Testing Scripts

**Purpose:** Early iteration testing scripts created during DevTools development

## 📁 Files

### `quick_screenshot_test.py`
- **Purpose:** Quick DevTools screenshot test
- **Features:** Basic daemon startup and screenshot capture
- **Status:** Early prototype - use main demos instead

### `test_direct_devtools.py` 
- **Purpose:** Direct DevTools Protocol connection testing
- **Features:** Raw WebSocket connection, Page.enable, Page.captureScreenshot
- **Status:** Low-level testing - main demos are more complete

### `test_screenshot.py`
- **Purpose:** Screenshot capture testing via DevTools daemon
- **Features:** Daemon connection testing with detailed logging
- **Status:** Development iteration - main demos are production-ready

## 🎯 Recommendation

**Use the main demo scripts in the parent directory instead:**
- `start_devtools_system.py` - Complete system automation
- `realtime_devtools_demo.py` - Real-time monitoring
- `continuous_devtools_demo.py` - Persistence proof
- `demo_devtools.py` - Step-by-step demonstration
- `trust_the_process.py` - Production workflow

These testing scripts are kept for historical reference but the main demos are more complete and production-ready.