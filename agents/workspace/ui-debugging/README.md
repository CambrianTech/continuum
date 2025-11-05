# UI Debugging Tools

Comprehensive toolkit for diagnosing and fixing Continuum UI issues, component loading problems, and JavaScript errors.

## 🎯 Current Status (Updated Jun 8, 2025)

**MAJOR BREAKTHROUGH**: Successfully resolved component loading issues!

### ✅ Recently Completed:
- **Fixed UIGenerator script paths**: Updated from `/ui/components/` to `/src/ui/components/`
- **Resolved template literal syntax errors**: Fixed unescaped backticks breaking HTML generation
- **Fixed server caching issues**: Properly terminated old server processes 
- **All components now loading**: ChatHeader, ChatArea, RoomTabs, StatusPill, AcademySection all served successfully
- **Version sync working**: Client and server versions properly aligned with auto-increment
- **Search debouncing implemented**: 300ms timeout added to AgentSelector search

### ✅ **COMPLETED - All Major Issues Resolved!**
- **Duplicate AgentSelector rendering**: ✅ FIXED - Single clean AgentSelector rendering
- **Component loading**: ✅ FIXED - All 6 components loading successfully  
- **Version sync**: ✅ FIXED - Auto-incrementing versions working properly
- **Left sidebar styling**: ✅ FIXED - Consistent styling with glass submenu system
- **Search debouncing**: ✅ IMPLEMENTED - 300ms timeout working

### 🔧 Minor Issues Remaining:
- **WebSocket client connection**: Python clients can't connect (separate from browser functionality)
- **Screenshot automation**: Need to resolve WebSocket client issue for automated screenshots

### 📋 Next Steps:
1. ~~Investigate duplicate AgentSelector rendering issue~~ ✅ COMPLETED
2. ~~Fix component loading and script paths~~ ✅ COMPLETED  
3. ~~Test all UI components are functioning correctly~~ ✅ COMPLETED
4. ~~Verify left sidebar styling consistency~~ ✅ COMPLETED
5. **Optional**: Investigate WebSocket client connectivity for automation tools

## 🔧 Available Tools

### 📊 `check_js_console_errors.py`
**Purpose**: Comprehensive JavaScript console error checker and system diagnostics.

**What it does**:
- Captures console errors, warnings, and logs
- Checks for missing UI components (chat-area, chat-header, room-tabs, status-pill)
- Tests browser capabilities (WebSocket, shadowDOM, customElements, etc.)
- Validates tab registration and WebSocket connection
- Provides detailed diagnostic information

**When to use**: 
- First step when UI isn't working properly
- Diagnosing component loading issues
- Checking WebSocket connectivity
- Verifying browser compatibility

**Example output**:
```
📊 RESULTS:
   • Errors found: 0
   • Warnings found: 0  
   • Issues detected: 5

⚠️  ISSUES FOUND:
   1. Missing component: chat-area
   2. Missing component: chat-header
   ...
```

---

### 🔍 `debug_component_loading.py`
**Purpose**: Deep dive into component script loading and registration issues.

**What it does**:
- Analyzes all loaded script tags and their sources
- Identifies missing expected component scripts
- Checks custom element registration status
- Tests component definition and DOM presence
- Provides script loading diagnostics

**When to use**:
- When `check_js_console_errors.py` shows missing components
- Debugging script path resolution issues
- Investigating component definition failures
- After changing component file paths

**Example output**:
```
📊 SCRIPT LOADING:
   • Total scripts: 5
   ❌ Missing scripts:
      - ChatHeader.js
      - ChatArea.js
      ...
```

---

### 🔄 `force_refresh_and_check.py`
**Purpose**: Force browser cache refresh and verify component loading after changes.

**What it does**:
- Forces hard page refresh to clear browser cache
- Waits for page reload to complete
- Re-checks component loading status
- Compares expected vs actual script paths
- Validates component functionality after refresh

**When to use**:
- After making changes to UIGenerator or component paths
- When browser cache might be interfering
- Testing if fixes are working after code changes
- Verifying new script paths are loading

**Example output**:
```
📊 RESULTS AFTER REFRESH:
   • Total scripts loaded: 5
   • Expected scripts found: 0
   • Expected scripts missing: 5
```

---

### 🎯 `fix_browser_tab_management.py`
**Purpose**: Test and diagnose browser tab management and focus issues.

**What it does**:
- Tests current tab detection and registration
- Evaluates browser focus capabilities
- Checks WebSocket connection status
- Analyzes tab visibility and focus state
- Provides recommendations for tab management improvements

**When to use**:
- When `continuum --restart` opens new tabs instead of focusing existing ones
- Debugging AppleScript permissions issues
- Testing browser automation capabilities
- Investigating tab registration problems

**Example output**:
```
📊 Current tab info:
   📊 URL: http://localhost:9000/
   📊 Focused: False
   📊 Visibility: hidden
   📊 WebSocket: True
```

---

### 🍎 `test_applescript_tab_detection.py`
**Purpose**: Test AppleScript browser automation and tab detection capabilities.

**What it does**:
- Tests Opera and Chrome tab detection via AppleScript
- Attempts to focus localhost:9000 tabs
- Checks running browser detection
- Diagnoses AppleScript permission issues
- Provides troubleshooting guidance

**When to use**:
- When browser tab focusing isn't working on macOS
- Debugging AppleScript permissions
- Testing cross-browser tab management
- Investigating browser automation failures

**Example output**:
```
🔍 Testing AppleScript Browser Tab Detection
🟠 Testing Opera tab detection...
   ❌ Opera error: Can't get application "Opera". (-1728)
🔵 Testing Chrome tab detection...
   📊 Chrome URLs found: 
   ❌ localhost:9000 not found in Chrome
```

## 🔗 Workflow Integration

### Typical Debugging Sequence

1. **Start with Overview**: `python check_js_console_errors.py`
   - Identifies high-level issues and missing components

2. **Deep Dive Components**: `python debug_component_loading.py`  
   - Analyzes script loading and component registration

3. **Test Changes**: `python force_refresh_and_check.py`
   - Verifies fixes after making code changes

4. **Browser Management**: `python fix_browser_tab_management.py`
   - Tests tab management and focus behavior

5. **AppleScript Issues**: `python test_applescript_tab_detection.py`
   - macOS-specific browser automation diagnostics

### Integration with Main Examples

These tools complement the main development workflow in [`python-client/examples/fix_ui_styling_with_feedback.py`](../../../python-client/examples/fix_ui_styling_with_feedback.py):

```python
# 1. Use UI debugging tools to identify issues
# 2. Apply fixes with live CSS injection  
# 3. Use debugging tools to verify fixes
# 4. Commit working fixes to source code
```

## 🎯 Common Issues and Solutions

### Missing Components
**Symptoms**: `check_js_console_errors.py` shows missing components
**Solution**: Use `debug_component_loading.py` to check script paths
**Fix**: Update UIGenerator script paths or verify file existence

### Script Loading Failures
**Symptoms**: 404 errors in browser Network tab
**Solution**: Check if files exist and paths are correct in UIGenerator
**Fix**: Update script src paths or move component files

### Browser Cache Issues  
**Symptoms**: Changes not appearing after code updates
**Solution**: Use `force_refresh_and_check.py` to clear cache
**Fix**: Force refresh or use incognito mode for testing

### Tab Management Problems
**Symptoms**: `continuum --restart` opens new tabs
**Solution**: Use `fix_browser_tab_management.py` and `test_applescript_tab_detection.py`
**Fix**: Check AppleScript permissions or improve tab detection logic

## 📚 Adding New Tools

When creating new debugging tools:

1. **Follow naming convention**: `action_subject_type.py`
2. **Include comprehensive docstring** explaining purpose and usage
3. **Add error handling** for connection and execution failures  
4. **Use consistent output formatting** with emojis and clear sections
5. **Update this README** with tool description and usage examples

---

*These tools enable rapid, systematic debugging of UI issues and provide the foundation for agent-driven development workflows.*