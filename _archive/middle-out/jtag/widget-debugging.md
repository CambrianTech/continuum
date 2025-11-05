# JTAG Widget Debugging System

## 🛸 **AI Autonomous Widget Analysis**

The JTAG system provides real-time browser widget analysis for autonomous AI debugging. It enables AIs to diagnose widget rendering issues, shadow DOM problems, and UI component states without manual intervention.

## 🎯 **Target Widget Architecture**

Based on `/screenshots/` references, the target interface includes:

### **Sidebar Layout Structure:**
- **Academy Status Widget** - "Academy Ready" button with training status
- **Session Costs Widget** - "$0.0000" display with request counter  
- **Active Projects Widget** - Collapsible project management section
- **Users & Agents Widget** - Main collapsible section with AI personas:
  - Claude Code (AI Assistant)
  - joel (Project Owner)
  - Auto Route (Smart agent selection)
  - CodeAI (Code analysis & debugging)
  - GeneralAI (General assistance)
  - PlannerAI (Strategy & web commands)

### **Main Interface:**
- **Chat Widget** - Full conversation interface with AI coordination
- **Version Widget** - Version display (e.g., "v0.2.2633")
- **Academy Training Panel** - Detailed AI persona training recommendations

## 🚀 **JTAG Usage Workflow**

### **Method 1: Full System + Analysis**
```bash
npm jtag
```
- Compiles, builds, starts daemons, opens browser
- Shows JTAG command at completion
- Use when starting fresh or after code changes

### **Method 2: Analysis Only (System Running)**  
```bash
./jtag widgets
```
- Shows JTAG commands (assumes system running)
- No browser launching or system restart
- Use for iterative debugging

### **Method 3: Custom Analysis**
```bash
./jtag probe
```
- Lists all available `window.jtag.*` methods
- Helps with specialized debugging scenarios

## 🔍 **Browser JTAG Commands**

Copy these into Browser DevTools (F12):

### **Complete Widget Analysis**
```javascript
fetch("/src/ui/jtag-probe.js").then(r=>r.text()).then(eval)
```
**Provides:**
- Widget count and rendering status
- Shadow DOM content analysis  
- Style loading verification
- Detailed content previews
- Issue identification with suggestions

### **Direct JTAG API Usage**
```javascript
// Widget shadow DOM analysis
window.jtag.widgets({ autoLog: true })

// Shadow DOM structure details
window.jtag.shadowDOM()

// System health check
window.jtag.health()

// Network connectivity status
window.jtag.network()

// Performance metrics
window.jtag.performance()
```

## 📊 **Understanding JTAG Results**

### **Widget Status Indicators:**
- **✅ Rendered** - Widget has shadow DOM content > 100 chars, visible, styled
- **⚠️ Empty** - Widget has shadow root but minimal/no content
- **❌ Broken** - Widget missing shadow root or has errors

### **Common Issues & Solutions:**

| Issue | Symptom | Solution |
|-------|---------|----------|
| **API Dependency** | "Failed to load X" errors | Create missing command stub or data daemon |
| **Empty Shadow DOM** | 0-50 chars content | Check `renderContent()` method implementation |
| **Missing Styles** | No `<style>` tags in shadow | Verify CSS asset loading in widget manifest |
| **Not Visible** | `offsetWidth/Height = 0` | Check parent container styles and positioning |

## 🔧 **Widget Development Patterns**

### **Current Widget Discovery (11 widgets):**
- ✅ **SidebarWidget** (continuum-sidebar) - Main navigation structure
- ✅ **ChatWidget** (chat-widget) - Communication interface  
- ✅ **SavedPersonas** (savedpersonas-widget) - AI personas list
- ✅ **ActiveProjectsWidget** (activeprojects-widget) - Project management
- ✅ **VersionWidget** (version-widget) - Version display
- ⚠️ **Academy, UserSelector** - Missing implementation files

### **Widget Architecture Requirements:**
1. **Shadow DOM rendering** - All widgets must implement `renderContent()`
2. **CSS asset loading** - Declarative asset system via BaseWidget
3. **API graceful degradation** - Mock data fallback when commands missing
4. **Modular structure** - Package.json + self-contained tests
5. **Event system integration** - Widget server controls for AI coordination

## 🎯 **AI Development Workflow**

1. **Start with JTAG analysis** - `npm jtag` → DevTools command
2. **Identify broken widgets** - Focus on empty/error widgets first  
3. **Check API dependencies** - Create missing command stubs
4. **Verify shadow DOM content** - Ensure `renderContent()` produces HTML
5. **Test iteratively** - `./jtag widgets` for quick re-analysis
6. **Build toward target** - Match screenshot interface layout

## 📁 **JTAG File Structure**

```
src/ui/jtag-probe.js              # Browser analysis script
src/commands/development/jtag/    # Server-side JTAG command
src/shared/types/JTAGSharedTypes.ts # Cross-platform interfaces  
./jtag                           # CLI wrapper (no browser launching)
package.json: "jtag" script      # npm jtag workflow
```

## 🤖 **Autonomous AI Benefits**

- **Self-diagnosing** - AI can identify widget issues independently
- **Real-time feedback** - Immediate analysis without manual inspection
- **Pattern recognition** - Systematic identification of common widget problems
- **Modular debugging** - Focus on specific widgets vs. entire system
- **Visual validation** - Shadow DOM content verification for UI components

The JTAG system enables AIs to autonomously debug widgets with the same precision as human developers using DevTools, but with systematic analysis and pattern recognition capabilities.