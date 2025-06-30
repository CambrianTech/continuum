#!/usr/bin/env python3
"""
Dynamic Continuum API Demo
Shows clean Python integration with command discovery
"""

import continuum_client

def main():
    print("🐍 Continuum Dynamic Python API Demo")
    print("=" * 40)
    
    # Single connect() call - auto-discovers everything
    continuum = continuum_client.connect()
    
    print("\n📋 Available Commands:")
    commands = continuum.list_commands()
    for cmd_name, cmd_info in commands.items():
        desc = cmd_info.get('description', 'No description')
        print(f"  • {cmd_name}: {desc}")
    
    print("\n🔍 Testing Dynamic Command Discovery:")
    
    # These methods are discovered automatically from the running system
    try:
        print("  ✅ continuum.health()")
        health = continuum.health()
        print(f"     Result: {health}")
    except Exception as e:
        print(f"     ❌ Failed: {e}")
    
    try:
        print("  ✅ continuum.daemon_status()")
        status = continuum.daemon_status()
        print(f"     Result: {len(status)} daemons")
    except Exception as e:
        print(f"     ❌ Failed: {e}")
    
    try:
        print("  ✅ continuum.console_logs()")
        logs = continuum.console_logs()
        print(f"     Result: {len(logs) if logs else 0} log entries")
    except Exception as e:
        print(f"     ❌ Failed: {e}")
    
    # Snake_case automatically converts to kebab-case
    try:
        print("  ✅ continuum.projects_list() (snake_case → projects-list)")
        projects = continuum.projects_list()
        print(f"     Result: {projects}")
    except Exception as e:
        print(f"     ❌ Failed: {e}")
    
    # JTAG browser hooks
    try:
        print("  ✅ continuum.browser_screenshot() (JTAG hook)")
        screenshot = continuum.browser_screenshot()
        if screenshot:
            print(f"     Result: Screenshot captured ({len(screenshot)} bytes)")
        else:
            print("     Result: No screenshot available")
    except Exception as e:
        print(f"     ❌ Failed: {e}")
    
    print("\n🎯 Key Benefits:")
    print("  • No manual API calls - methods discovered automatically")
    print("  • Python-friendly naming (snake_case)")
    print("  • JTAG hooks for autonomous development")
    print("  • Auto-start if Continuum not running")
    print("  • Type hints and docstrings from command metadata")
    
    print("\n✨ Usage Examples:")
    print("  continuum.health()                    # System health")
    print("  continuum.deploy(target='prod')       # Deploy with args")
    print("  continuum.preferences_set(theme='dark') # Preferences")
    print("  continuum.console_logs()              # JTAG console")
    print("  continuum.browser_screenshot()        # JTAG browser")
    print("  continuum.daemon_restart('renderer')  # JTAG daemon control")

if __name__ == "__main__":
    main()