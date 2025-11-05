# UI Styling & Debugging Tools

This directory contains reusable utilities for debugging and fixing UI styling issues in Continuum components. These tools provide visual before/after validation and automated CSS fix application.

## 🎨 UIStylingDebugger (`ui_styling_debugger.py`)

**Purpose**: Comprehensive before/after visual debugging tool for UI styling fixes.

### Features
- 📸 **Before/After Screenshots** - Automatic capture of styling changes
- 🎯 **Component Targeting** - Works with both shadow DOM and regular DOM elements
- 🔧 **CSS Fix Application** - Apply styling fixes and validate results
- 📁 **Organized Output** - Timestamped screenshots in `.continuum/screenshots/`
- ✅ **Verification Mode** - Reload page and verify permanent fixes

### Usage

```python
from ui_styling_debugger import UIStylingDebugger

# Create debugger instance
debugger = UIStylingDebugger("component_name")

# Apply CSS fixes with before/after capture
css_fixes = """
.search-input {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
}
"""

result = await debugger.capture_before_after_styling_fix(
    client,
    'agent-selector',  # CSS selector
    css_fixes,
    "search box styling fixes"
)

# Optional: Verify permanent fixes after page reload
verified_path = await debugger.reload_and_verify(
    client, 'agent-selector', "permanent styling verification"
)
```

### Output
- `component_BEFORE_timestamp.png` - Before screenshot
- `component_AFTER_timestamp.png` - After screenshot  
- `component_VERIFIED_timestamp.png` - Verification screenshot (optional)

## 🔧 ComponentCSSFixer (`component_css_fixer.py`)

**Purpose**: Quick CSS fix application tool for immediate styling corrections.

### Features
- ⚡ **Quick Fixes** - Apply CSS rules to components instantly
- 🌐 **Shadow DOM Support** - Works with web components and regular DOM
- 📋 **Pre-defined Fixes** - Common styling solutions ready to use
- 🎯 **Targeted Application** - Apply fixes to specific components
- 🔍 **CLI Interface** - Command-line usage for quick fixes

### Usage

#### Programmatic Usage
```python
from component_css_fixer import ComponentCSSFixer

# Quick fix application
await ComponentCSSFixer.quick_style_fix(
    'agent-selector',
    '.title { font-weight: 600; letter-spacing: 1px; }',
    'Fix title typography'
)

# Apply multiple fixes
fixer = ComponentCSSFixer()
await fixer.apply_css_to_component(
    client, 'agent-selector', css_rules, 'description'
)
```

#### Command Line Usage
```bash
# Fix USERS & AGENTS section
python component_css_fixer.py --users-agents

# Fix glass submenu positioning
python component_css_fixer.py --glass-submenu

# Apply all common fixes
python component_css_fixer.py --all
```

### Pre-defined Fix Collections

#### `CommonCSSFixes.SEARCH_BOX_FIXES`
- Search input styling with proper background and borders
- Focus states with blue glow effects
- Icon positioning and placeholder styling

#### `CommonCSSFixes.TITLE_FIXES`
- Professional typography for section titles
- Consistent letter spacing and font weight
- Proper margins and text transformation

#### `CommonCSSFixes.FAVORITE_BUTTON_FIXES`
- Clean favorite button positioning
- Hover states and action button layouts
- Removal of clashing visual elements

#### `CommonCSSFixes.GLASS_SUBMENU_POSITIONING`
- Fixed positioning for glass submenu panels
- Backdrop blur and transparency effects
- Proper z-index and shadow styling

## 🛠️ Common Workflows

### 1. Debug New Styling Issue
```python
# Use UIStylingDebugger for comprehensive before/after analysis
debugger = UIStylingDebugger("new_component")
result = await debugger.capture_before_after_styling_fix(
    client, '.problematic-element', css_fixes, "description"
)
```

### 2. Apply Quick Fix
```python
# Use ComponentCSSFixer for immediate corrections
await ComponentCSSFixer.quick_style_fix(
    '.broken-element', 
    'background: rgba(255,255,255,0.1);',
    'Fix background opacity'
)
```

### 3. Systematic Component Overhaul
```python
# Combine both tools for comprehensive fixes
# 1. Debug with before/after captures
# 2. Apply permanent fixes to source code
# 3. Verify with reload testing
```

## 📁 File Organization

```
python-client/examples/
├── ui_styling_debugger.py       # Comprehensive visual debugging
├── component_css_fixer.py       # Quick CSS fix application
├── README_UI_STYLING_TOOLS.md   # This documentation
├── natural_glass_submenu_demo.py # Glass submenu demo (example usage)
└── README_glass_submenu_demo.md # Glass submenu documentation
```

## 🎯 Example: USERS & AGENTS Section Fix

### Problem
- Unstyled search box 
- Poor title typography
- Clashing favorite star positioning

### Solution Using Tools
```python
# Step 1: Visual debugging
debugger = UIStylingDebugger("users_agents")
css_fixes = CommonCSSFixes.SEARCH_BOX_FIXES + CommonCSSFixes.TITLE_FIXES
result = await debugger.capture_before_after_styling_fix(
    client, 'agent-selector', css_fixes, "USERS & AGENTS fixes"
)

# Step 2: Apply permanent fixes to AgentSelector.js component
# (Manual step - edit the component source file)

# Step 3: Verify permanent fixes
verified = await debugger.reload_and_verify(
    client, 'agent-selector', "permanent fix verification"
)
```

### Results
- ✅ Professional search box styling
- ✅ Consistent title typography  
- ✅ Clean favorite button positioning
- ✅ Visual documentation of improvements

## 🚀 Best Practices

### 1. **Always Capture Before Screenshots**
- Document the original problem state
- Provides visual proof of improvements
- Helps with debugging regressions

### 2. **Use Descriptive Names**
- Clear component names and fix descriptions
- Organized screenshot naming with timestamps
- Meaningful CSS rule descriptions

### 3. **Verify Permanent Fixes**
- Use reload verification for persistent changes
- Test that source code changes work correctly
- Ensure fixes survive page refreshes

### 4. **Combine Tools Strategically**
- UIStylingDebugger for new issues and comprehensive fixes
- ComponentCSSFixer for quick corrections and testing
- Both tools for systematic component overhauls

### 5. **Document Results**
- Save before/after screenshots for reference
- Commit source code changes with visual evidence
- Create documentation for future maintenance

## 🔍 Troubleshooting

### Shadow DOM Components
- Tools automatically detect shadow DOM vs regular DOM
- CSS fixes are applied to the correct context
- Use browser dev tools to verify shadow root structure

### Screenshot Failures
- Ensure html2canvas is loaded in the browser
- Check that target elements exist and are visible
- Verify WebSocket connection is stable

### CSS Application Issues
- Check browser console for JavaScript errors
- Verify CSS syntax is valid
- Test with simpler CSS rules first

## 📚 Related Documentation

- **Main README**: `../README.md` - Python client overview
- **Glass Submenu Demo**: `README_glass_submenu_demo.md` - Specific demo documentation
- **Screenshot Examples**: `simple_screenshot.py`, `find_and_capture.py` - Basic screenshot tools
- **Architecture**: `../../ARCHITECTURE.md` - System architecture overview

---

**💡 Tip**: These tools serve as "visual debugging macros" that provide automated before/after validation for UI styling work. They're designed to be reusable across different components and styling issues.