# Contributing to Agent Scripts

## Adding New Tools

### Python Tools (`tools/python/`)

When adding new Python tools:

1. **Follow the pattern** - Look at `js-send.py` and `heal.py` as templates
2. **Use Click framework** - For consistent CLI interfaces
3. **Add to requirements.txt** - Include any new dependencies
4. **Create wrapper in bin/** - For auto-venv execution
5. **Document in docs/** - Update ARCHITECTURE.md and EXAMPLES.md

#### Template for New Python Tool

```python
#!/usr/bin/env python3
"""
Tool Name - Brief Description
"""

import click
import warnings

# Suppress urllib3 SSL warnings
warnings.filterwarnings('ignore', message='urllib3 v2 only supports OpenSSL 1.1.1+')

@click.command()
@click.argument('input_param', required=True)
@click.option('--quiet', is_flag=True, help='Minimal output')
@click.option('--json', 'json_only', is_flag=True, help='JSON output only')
def main(input_param, quiet, json_only):
    """
    Tool description and usage examples.
    """
    try:
        # Tool logic here
        result = {"success": True, "message": "Tool executed"}
        
        if json_only:
            click.echo(json.dumps(result))
        elif quiet:
            click.echo("✅ SUCCESS" if result['success'] else "❌ FAILED")
        else:
            click.echo(f"🎯 {result['message']}")
            
    except Exception as e:
        if json_only:
            click.echo(json.dumps({"success": False, "error": str(e)}))
        else:
            click.echo(f"❌ Error: {e}")

if __name__ == '__main__':
    main()
```

### JavaScript Examples (`examples/`)

When adding JavaScript examples:

1. **Choose appropriate category** - jokes/, diagnostics/, fixes/, etc.
2. **Include descriptive comments** - Explain what the script does
3. **Add status logging** - Use console.log for progress indication
4. **Test thoroughly** - Ensure no syntax errors or infinite loops
5. **Document in EXAMPLES.md** - Add usage examples

#### Template for JavaScript Examples

```javascript
// examples/category/my-script.js
console.log("🎯 Starting [Script Name]...");

// Brief description of what this script does
// and why it's useful as an example

try {
    // Main script logic here
    const result = document.querySelector('#target');
    
    if (result) {
        console.log("✅ Found target element:", result);
        // Perform operations
    } else {
        console.log("ℹ️ Target element not found - that's okay");
    }
    
    console.log("✅ [Script Name] completed successfully");
    
} catch (error) {
    console.error("❌ [Script Name] failed:", error.message);
    console.error("📚 Stack trace:", error.stack);
}
```

## Categories for Examples

### Current Categories

- **jokes/** - Humor/demonstration scripts that show basic functionality
- **diagnostics/** - System analysis and debugging tools  
- **fixes/** - Browser/console issue resolution scripts

### Suggested New Categories

- **automation/** - UI automation and testing scripts
- **performance/** - Performance monitoring and optimization
- **accessibility/** - A11y testing and analysis tools
- **security/** - Security scanning and analysis
- **development/** - Developer productivity tools

## Testing Guidelines

### Before Submitting

1. **Test with js-send** - Verify script executes without errors
2. **Check console output** - Ensure clean telemetry
3. **Test edge cases** - Handle missing elements gracefully
4. **Verify auto-healing** - Ensure doesn't break probe communication
5. **Update documentation** - Add to appropriate .md files

### Testing Commands

```bash
# Test basic execution
js-send examples/category/new-script.js

# Test quiet mode
js-send --quiet examples/category/new-script.js

# Test JSON output
js-send --json examples/category/new-script.js

# Test error handling
js-send 'throw new Error("test error")'
```

## Code Style

### Python
- Follow PEP 8 style guide
- Use type hints where helpful
- Include docstrings for functions
- Use Click for CLI interfaces
- Handle errors gracefully

### JavaScript  
- Use modern ES6+ syntax
- Include descriptive console logging
- Handle errors with try/catch
- Avoid infinite loops or blocking operations
- Keep scripts focused and single-purpose

## Pull Request Process

1. **Test thoroughly** - All scripts must work with current system
2. **Update documentation** - Include relevant .md file updates
3. **Follow naming conventions** - Use lowercase-with-hyphens
4. **Include examples** - Show how to use new functionality
5. **Consider auto-healing** - Ensure new tools don't break probe communication

## Future Directions

### Planned Enhancements

- **Multi-language support** - Add tools/javascript/, tools/bash/, etc.
- **Testing framework** - Automated testing for examples
- **GUI interface** - Web-based agent control panel
- **Remote probe support** - Control browsers on different machines
- **Plugin system** - Modular tool loading

### Architecture Improvements

- **Better error recovery** - More sophisticated healing patterns
- **Performance monitoring** - Built-in performance analysis
- **Security scanning** - Automated security checks
- **Cross-browser testing** - Multi-browser probe support

This contributing guide ensures consistent, high-quality additions to the agent scripts ecosystem.