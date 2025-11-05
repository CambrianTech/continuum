# Agents Workspace

This directory contains organized agent scripts and tools for debugging, fixing, and maintaining Continuum.

## 📁 Directory Structure

### `/ui-debugging/`
Tools for diagnosing and fixing UI component issues, JavaScript errors, and browser compatibility problems.

### `/browser-management/` 
Scripts for testing and improving browser tab management, AppleScript automation, and cross-platform compatibility.

### `/screenshots/`
Screenshot capture tools and visual debugging utilities for UI development workflows.

## 🎯 Purpose

The agents workspace provides:

1. **Organized Debugging Tools** - Categorized scripts for specific problem domains
2. **Reusable Utilities** - Common functions that can be imported across scripts  
3. **Documentation** - README files explaining each tool and workflow
4. **Examples** - Reference implementations for common debugging patterns

## 🔧 Usage Patterns

### For UI Issues
```bash
cd agents/workspace/ui-debugging
python check_js_console_errors.py     # Check for JavaScript errors
python debug_component_loading.py     # Debug missing components
python force_refresh_and_check.py     # Test after changes
```

### For Browser Issues  
```bash
cd agents/workspace/browser-management
python test_applescript_tab_detection.py  # Test tab management
python fix_browser_tab_management.py      # Fix restart behavior
```

### For Screenshot Workflows
```bash
cd agents/workspace/screenshots
python capture_before_after.py        # Before/after comparisons
python visual_diff_checker.py         # Visual change detection
```

## 📚 Integration with Documentation

These tools should be referenced in:
- [`docs/AGENT_DEVELOPMENT_GUIDE.md`](../../docs/AGENT_DEVELOPMENT_GUIDE.md) - Main development guide
- [`python-client/examples/README.md`](../../python-client/examples/README.md) - Example workflows
- [`agent-scripts/README.md`](../../agent-scripts/README.md) - Agent automation

## 🎯 Best Practices

1. **Clear Naming** - Scripts should have descriptive names indicating their purpose
2. **Good Documentation** - Each script should have a header explaining what it does
3. **Modular Design** - Reusable functions should be extracted to utility modules
4. **Error Handling** - Scripts should gracefully handle connection and execution errors
5. **Output Formatting** - Use consistent emoji and formatting for readability

## 🔄 Development Workflow

1. **Identify Issue** - Use diagnostic scripts to understand the problem
2. **Create Fix** - Develop targeted solution scripts
3. **Test Changes** - Use verification scripts to confirm fixes
4. **Document** - Update READMEs and add to agent documentation
5. **Integrate** - Move successful patterns to main examples

---

*This workspace is designed to make agent-driven development more organized and efficient.*