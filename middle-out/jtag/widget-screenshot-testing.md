# Widget Screenshot Testing Guide

## 🎯 **Visual Widget Validation with JTAG**

### **Overview**
The JTAG screenshot system provides comprehensive visual validation for widget development and debugging. This guide covers how to use verified selectors for widget testing and development feedback.

## 📸 **Verified Widget Selectors**

### **Core UI Components**
These selectors are verified to work with the screenshot system:

#### **`chat-widget`** - Chat Interface Component
```bash
# Capture chat widget state
./continuum screenshot --querySelector="chat-widget" --filename="chat-widget-debug.png"

# Test chat widget after changes
./continuum screenshot --querySelector="chat-widget" --filename="chat-widget-after-fix.png"
```

#### **`continuum-sidebar`** - Main Sidebar Navigation
```bash
# Capture sidebar state
./continuum screenshot --querySelector="continuum-sidebar" --filename="sidebar-debug.png"

# Test sidebar responsiveness
./continuum screenshot --querySelector="continuum-sidebar" --filename="sidebar-responsive.png"
```

#### **`.app-container`** - Main Application Container
```bash
# Capture full app layout
./continuum screenshot --querySelector=".app-container" --filename="app-layout-debug.png"

# Test layout changes
./continuum screenshot --querySelector=".app-container" --filename="app-layout-after-changes.png"
```

#### **`body`** - Full Page Capture
```bash
# Full page screenshot
./continuum screenshot --querySelector="body" --filename="full-page-debug.png"

# Complete UI state validation
./continuum screenshot --querySelector="body" --filename="ui-state-validation.png"
```

#### **`div`** - Generic Container Elements
```bash
# First div element (useful for quick debugging)
./continuum screenshot --querySelector="div" --filename="div-debug.png"
```

## 🔧 **Development Workflow Integration**

### **Widget Development Cycle**
1. **Make widget changes** - Edit widget code
2. **Rebuild browser bundle** - `npm run build:browser-ts`
3. **Capture before state** - Screenshot current state
4. **Test functionality** - Interact with widget
5. **Capture after state** - Screenshot final state
6. **Compare visually** - Validate changes

### **Example Development Session**
```bash
# Start development
npm start

# Make widget changes
# ... edit widget code ...

# Rebuild browser bundle
npm run build:browser-ts

# Capture current state
./continuum screenshot --querySelector="chat-widget" --filename="chat-before-fix.png"

# Test your changes
# ... interact with widget ...

# Capture after state
./continuum screenshot --querySelector="chat-widget" --filename="chat-after-fix.png"

# Full page validation
./continuum screenshot --querySelector="body" --filename="full-page-after-fix.png"
```

## 📁 **Screenshot Storage Structure**

### **Session-Based Organization**
All screenshots are organized by session:

```
.continuum/sessions/user/shared/{SESSION_ID}/screenshots/
├── chat-widget-debug.png
├── sidebar-debug.png
├── app-layout-debug.png
├── full-page-debug.png
├── chat-before-fix.png
├── chat-after-fix.png
└── ui-state-validation.png
```

### **Finding Your Session ID**
```bash
# Get current session info
./continuum session-info

# Or look for most recent session
ls -la .continuum/sessions/user/shared/
```

## 🎨 **Widget-Specific Testing Patterns**

### **Chat Widget Testing**
```bash
# Basic chat widget state
./continuum screenshot --querySelector="chat-widget" --filename="chat-idle.png"

# Chat with messages
# ... send test message ...
./continuum screenshot --querySelector="chat-widget" --filename="chat-with-messages.png"

# Chat error state
# ... trigger error condition ...
./continuum screenshot --querySelector="chat-widget" --filename="chat-error-state.png"
```

### **Sidebar Widget Testing**
```bash
# Sidebar default state
./continuum screenshot --querySelector="continuum-sidebar" --filename="sidebar-default.png"

# Sidebar with different tab
# ... switch to Academy tab ...
./continuum screenshot --querySelector="continuum-sidebar" --filename="sidebar-academy-tab.png"

# Sidebar with user interactions
# ... interact with user elements ...
./continuum screenshot --querySelector="continuum-sidebar" --filename="sidebar-user-interactions.png"
```

## 🚀 **Advanced Testing Scenarios**

### **Responsive Design Testing**
```bash
# Test different viewport sizes (manual browser resize)
./continuum screenshot --querySelector="body" --filename="responsive-desktop.png"

# After manual resize
./continuum screenshot --querySelector="body" --filename="responsive-mobile.png"
```

### **Error State Documentation**
```bash
# Capture error states for debugging
./continuum screenshot --querySelector="chat-widget" --filename="error-no-connection.png"
./continuum screenshot --querySelector="continuum-sidebar" --filename="error-loading-users.png"
```

### **Before/After Comparison**
```bash
# Before making changes
./continuum screenshot --querySelector="body" --filename="before-ui-update.png"

# Make changes...
# ... edit code, rebuild ...

# After changes
./continuum screenshot --querySelector="body" --filename="after-ui-update.png"
```

## 🔍 **Debugging with Screenshots**

### **Visual Debugging Process**
1. **Identify issue** - Notice UI problem
2. **Capture current state** - Screenshot problematic area
3. **Check logs** - Review browser.log and server.log
4. **Make fix** - Edit code and rebuild
5. **Verify fix** - Screenshot after fix
6. **Document** - Save both screenshots for comparison

### **Common Debugging Scenarios**
```bash
# Widget not rendering
./continuum screenshot --querySelector="chat-widget" --filename="widget-not-rendering.png"

# Layout issues
./continuum screenshot --querySelector=".app-container" --filename="layout-broken.png"

# Style problems
./continuum screenshot --querySelector="continuum-sidebar" --filename="styles-broken.png"
```

## 📊 **Integration with Git Hooks**

### **Pre-commit Visual Validation**
The git hook system automatically captures screenshots during commits:

```bash
# Git hooks automatically run these:
./continuum screenshot --querySelector="chat-widget" --filename="pre-commit-chat.png"
./continuum screenshot --querySelector="continuum-sidebar" --filename="pre-commit-sidebar.png"
./continuum screenshot --querySelector="body" --filename="pre-commit-full.png"
```

### **Regression Detection**
Screenshots can be used to detect UI regressions:

1. **Baseline screenshots** - Capture known-good states
2. **Comparison screenshots** - Capture after changes
3. **Visual diff** - Compare for unexpected changes
4. **Automated validation** - Block commits with visual regressions

## 🎯 **Best Practices**

### **Naming Conventions**
- Use descriptive filenames: `chat-widget-loading-state.png`
- Include timestamp for comparison: `sidebar-2025-07-15-debug.png`
- Indicate purpose: `full-page-before-fix.png`, `full-page-after-fix.png`

### **Storage Management**
- Screenshots are session-scoped - they don't persist across sessions
- For permanent debugging images, copy to a dedicated folder
- Use git to track important baseline screenshots

### **Performance Considerations**
- Screenshots are captured client-side using html2canvas
- Large screenshots may take longer to process
- Consider using specific selectors instead of full-page captures for faster debugging

## 🔄 **Claude Development Integration**

### **AI-Friendly Screenshot Feedback**
Claude can now use screenshots for development feedback:

```bash
# Claude can run these commands to get visual feedback
./continuum screenshot --querySelector="chat-widget" --filename="claude-feedback.png"

# Then analyze the visual result to understand:
# - Widget rendering state
# - Layout issues
# - Style problems
# - Functionality verification
```

### **Iterative Development Pattern**
1. **Claude makes changes** - Edit widget code
2. **Rebuild system** - `npm run build:browser-ts`
3. **Capture screenshot** - Visual validation
4. **Analyze result** - Check if changes worked
5. **Iterate** - Repeat until satisfied

This creates a visual feedback loop that enables rapid, confident development with immediate visual confirmation of changes.

## 📚 **Related Documentation**

- **[JTAG README](README.md)** - Complete JTAG framework overview
- **[Widget Architecture](../development/widget-architecture.md)** - Widget system design
- **[Development Workflow](../development/README.md)** - Development methodologies
- **[Testing Strategy](../development/testing-workflow.md)** - Complete testing approach

---

**The screenshot testing system transforms widget development from blind coding to visual validation, enabling confident, rapid development with immediate feedback.**