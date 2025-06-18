#!/usr/bin/env python3
"""
AI Agent Dashboard - Quick Command Center for AI Agents
======================================================
Streamlined interface for AI agents to see what needs work and pick tickets.

Usage:
  python3 ai-agent.py                    # Full dashboard  
  python3 ai-agent.py --dashboard        # Full dashboard (explicit)
  python3 ai-agent.py --broken           # Just show broken items
  python3 ai-agent.py --recent           # Just show recent work
  python3 ai-agent.py --quick            # Quick status only
"""

import asyncio
import subprocess
import sys
import re
from pathlib import Path

async def run_command(cmd):
    """Run ai-portal command and return clean output"""
    try:
        # Get the directory where this script is located
        script_dir = Path(__file__).parent
        portal_path = script_dir / 'ai-portal.py'
        
        result = subprocess.run(
            ['python3', str(portal_path)] + cmd.split()[1:], 
            capture_output=True, 
            text=True,
            cwd=script_dir
        )
        return result.returncode == 0, result.stdout, result.stderr
    except Exception as e:
        return False, "", str(e)

def get_command_dependencies():
    """Get command dependencies from README files and fallback script"""
    dependencies = {}
    commands_dir = Path(__file__).parent.parent / 'src' / 'commands' / 'core'
    
    # Fallback known dependencies from the test scripts
    known_deps = {
        'exec': [],                           # No dependencies - foundation
        'filesave': [],                       # No dependencies - foundation  
        'clear': [],                          # No dependencies - foundation
        'workspace': [],                      # No dependencies - foundation
        'help': [],                           # No dependencies - foundation
        'info': [],                           # No dependencies - foundation
        'screenshot': ['exec', 'filesave'],   # Depends on exec and filesave
        'share': ['screenshot'],              # Depends on screenshot
        'diagnostics': ['exec', 'screenshot'], # Depends on exec and screenshot
        'browser': ['exec'],                  # Depends on exec
        'input': ['browser'],                 # Depends on browser functionality
        'cursor': ['browser'],                # Depends on browser functionality
        'type': ['browser'],                  # Depends on browser functionality
        'move': ['browser'],                  # Depends on browser functionality
        'emotion': ['exec'],                  # Depends on exec
        'chat': ['exec'],                     # Depends on exec
        'reload': ['browser'],                # Depends on browser
    }
    
    if not commands_dir.exists():
        return known_deps
    
    for cmd_dir in commands_dir.iterdir():
        if cmd_dir.is_dir():
            readme_path = cmd_dir / 'README.md'
            if readme_path.exists():
                try:
                    content = readme_path.read_text()
                    deps = []
                    
                    # Parse mermaid dependencies from README
                    in_mermaid = False
                    node_map = {}  # Map A -> CommandName
                    
                    for line in content.split('\n'):
                        if 'graph TD' in line or 'graph LR' in line:
                            in_mermaid = True
                            continue
                        elif in_mermaid and line.strip().startswith('```'):
                            break
                        elif in_mermaid and '[' in line and ']' in line:
                            # Parse node definitions like "A[InputCommand]"
                            import re
                            node_matches = re.findall(r'([A-Z])\[([^\]]+)\]', line)
                            for node_id, node_label in node_matches:
                                # Extract command name from label
                                if 'Command' in node_label:
                                    cmd_name = node_label.replace('Command', '').lower()
                                    node_map[node_id] = cmd_name
                                elif node_label in ['BaseCommand', 'DOM Manipulation', 'BrowserJSCommand']:
                                    # These are dependencies, map as-is
                                    dep_name = node_label.lower().replace(' ', '').replace('command', '')
                                    node_map[node_id] = dep_name
                        elif in_mermaid and '-->' in line:
                            # Parse A --> B format and resolve to actual command names
                            parts = line.split('-->')
                            if len(parts) >= 2:
                                source_node = parts[0].strip().split()[-1]  # Get last part (node ID)
                                target_node = parts[1].strip().split()[0]   # Get first part (node ID)
                                
                                # If this is the main command pointing to dependencies
                                if source_node in node_map and target_node in node_map:
                                    source_cmd = node_map[source_node]
                                    target_cmd = node_map[target_node]
                                    
                                    # Only add as dependency if source is current command
                                    if source_cmd == cmd_dir.name.lower() and target_cmd != source_cmd:
                                        if target_cmd not in ['basecommand', 'dommanipulation', 'browserjscommand']:
                                            deps.append(target_cmd)
                    
                    # Use parsed dependencies if found, otherwise fallback to known_deps
                    if deps:
                        dependencies[cmd_dir.name] = list(set(deps))  # Remove duplicates
                    else:
                        dependencies[cmd_dir.name] = known_deps.get(cmd_dir.name, [])
                except Exception:
                    dependencies[cmd_dir.name] = known_deps.get(cmd_dir.name, [])
    
    # Merge with known dependencies for any missing commands
    for cmd, deps in known_deps.items():
        if cmd not in dependencies:
            dependencies[cmd] = deps
    
    return dependencies

def topological_sort(dependencies):
    """Sort commands by dependencies using topological sort"""
    visited = set()
    result = []
    
    def visit(command):
        if command in visited:
            return
        visited.add(command)
        
        # Visit dependencies first
        deps = dependencies.get(command, [])
        for dep in deps:
            if dep in dependencies:  # Only visit if it's a known command
                visit(dep)
        
        # Add current command after its dependencies
        result.append(command)
    
    for command in dependencies.keys():
        visit(command)
    
    return result

def get_command_tickets():
    """Get all command tickets with status from README files, ranked by dependencies"""
    tickets = []
    commands_dir = Path(__file__).parent.parent / 'src' / 'commands' / 'core'
    
    if not commands_dir.exists():
        return tickets
    
    # Get dependency information
    dependencies = get_command_dependencies()
    dependency_order = topological_sort(dependencies)
    
    # Create order mapping for ranking
    order_map = {cmd: i for i, cmd in enumerate(dependency_order)}
    
    for cmd_dir in commands_dir.iterdir():
        if cmd_dir.is_dir():
            readme_path = cmd_dir / 'README.md'
            if readme_path.exists():
                try:
                    content = readme_path.read_text()
                    
                    # Parse status
                    status_match = re.search(r'\*\*Status\*\*:\s*([^\n]+)', content)
                    status = status_match.group(1) if status_match else '⚪ Unknown'
                    
                    # Get date from status
                    date_match = re.search(r'(\d{4}-\d{2}-\d{2})', status)
                    date = date_match.group(1) if date_match else '0000-00-00'
                    
                    # Count TODOs
                    todo_count = len(re.findall(r'TODO:', content))
                    
                    # Get issue description
                    issue = ""
                    if '🔴' in status:
                        parts = status.split(' - ', 1)
                        issue = parts[1] if len(parts) > 1 else "Needs investigation"
                    
                    # Determine priority
                    if '🔴' in status:
                        priority = 1  # Highest
                    elif '🟠' in status:
                        priority = 2  # High (untested)
                    elif '🟡' in status:
                        priority = 3  # Medium (testing)
                    elif '🟢' in status:
                        priority = 4  # Low (stable)
                    else:
                        priority = 5  # Lowest (unknown)
                    
                    # Get dependency order (lower = should fix first)
                    dep_order = order_map.get(cmd_dir.name, 999)
                    deps = dependencies.get(cmd_dir.name, [])
                    
                    tickets.append({
                        'name': cmd_dir.name,
                        'status': status,
                        'issue': issue,
                        'date': date,
                        'todos': todo_count,
                        'priority': priority,
                        'dep_order': dep_order,
                        'dependencies': deps
                    })
                except Exception:
                    # Skip files with parse errors
                    continue
    
    return tickets

async def show_dashboard():
    """Show comprehensive AI agent dashboard"""
    print("🤖 AI AGENT DASHBOARD")
    print("=" * 50)
    
    print("\n🎯 THE VISION:")
    print("   Building COMPLETELY AUTONOMOUS agent code generation with:")
    print("   • Full debug logs (client/server/browser console)")
    print("   • Screenshot capture for visual verification")
    print("   • Code execution monitoring via Sentinel bots")
    print("   • Organized task-based logging for continuity")
    print("   • Future: Sentinel agents running Continuum JS scripts autonomously")
    
    print("\n📖 READ THIS FIRST: python-client/ai-agent-README.md")
    print("   ^ Complete debugging workflow & current priorities")
    
    print("\n📋 COLLABORATION RULES:")
    print("   ✅ ALWAYS test commands before/after changes")
    print("   ✅ UPDATE README with what you learn (even if you don't fix)")
    print("   ✅ REFINE ROADMAP: Add/update features you discover are needed")
    print("   ✅ Leave breadcrumbs for the next person")
    print("   ✅ Small changes only (max 50 lines)")
    print("   ✅ Sync dashboard when done: python3 ai-portal.py --cmd docs")
    
    # Get current project status
    print("\n📊 PROJECT HEALTH:")
    success, output, _ = await run_command("--cmd agents")
    if success:
        # Extract just the health line
        lines = output.split('\n')
        for line in lines:
            if 'PROJECT STATUS:' in line:
                print(f"   {line.split('PROJECT STATUS:')[1].strip()}")
                break
    
    print("\n🧪 TESTING FIRST - CRITICAL:")
    print("   python3 ai-portal.py --cmd tests # 🚀 RUN ALL TESTS (one command!)")
    print("   ^ Do this BEFORE and AFTER any changes to verify system health")
    
    print("\n🎯 QUICK ACTIONS:")
    print("   python3 ai-agent.py --broken     # See what's broken in fix order")
    print("   python3 ai-agent.py --roadmap    # View roadmap & add your ideas") 
    print("   python3 ai-agent.py --files      # See file structure & reduction mission")
    print("   python3 ai-agent.py --logs       # View debugging logs with Sentinel") 
    print("   python3 ai-portal.py --cmd [cmd] # Test individual command")
    print("   python3 ai-portal.py --cmd docs  # Update main dashboard")
    
    print("\n📋 TICKET WORKFLOW:")
    print("   1. 🧪 FIRST: python3 ai-portal.py --cmd tests (verify baseline)")
    print("   2. Pick a 🔴 or 🟠 ticket below (fix in dependency order)")
    print("   3. Test command: python3 ai-portal.py --cmd [command-name]")
    print("   4. Fix & document findings in README")
    print("   5. 🧪 VERIFY: python3 ai-portal.py --cmd tests (confirm no regressions)")
    print("   6. Sync: python3 ai-portal.py --cmd docs")
    
    # Get actual tickets with dependency ranking
    tickets = get_command_tickets()
    broken_tickets = [t for t in tickets if '🔴' in t['status']]
    
    print(f"\n🚨 BROKEN COMMANDS ({len(broken_tickets)} high impact tickets):")
    if broken_tickets:
        # Sort by dependency order (fix foundation commands first), then priority
        broken_tickets.sort(key=lambda x: (x['dep_order'], x['priority']))
        for i, ticket in enumerate(broken_tickets[:5], 1):  # Show top 5
            deps_str = f" (needs: {', '.join(ticket['dependencies'])})" if ticket['dependencies'] else " (no deps)"
            print(f"   {i}. 🔴 {ticket['name']} - {ticket['issue']}{deps_str}")
    else:
        print("   🎉 No broken commands found!")
    
    # Show logical fix order
    if broken_tickets:
        print(f"\n🔧 LOGICAL FIX ORDER (dependency-first):")
        for i, ticket in enumerate(broken_tickets[:8], 1):
            fix_reason = "foundation" if not ticket['dependencies'] else f"depends on {', '.join(ticket['dependencies'])}"
            print(f"   {i}. {ticket['name']} - {fix_reason}")
    
    # Show untested commands  
    untested_tickets = [t for t in tickets if '🟠' in t['status']]
    if untested_tickets:
        print(f"\n🟠 UNTESTED COMMANDS ({len(untested_tickets)} exploration opportunities):")
        untested_tickets.sort(key=lambda x: x['dep_order'])  # Dependency order
        for ticket in untested_tickets[:3]:  # Show top 3
            print(f"   🟠 {ticket['name']} - Great for adding documentation")
    
    print("\n💡 TIP: Start with 🔴 broken commands for highest impact!")
    print("   Even 5 minutes of investigation helps the next person.")
    
    # Add roadmap section
    print("\n🗺️ DEVELOPMENT ROADMAP - Future Features:")
    print("   🧪 Sentinel Test Runners: One-button testing with preserved logs")
    print("   📊 Performance Metrics: Execution time, memory usage in dashboard")
    print("   🌐 Web Widget Dashboard: Browser-embedded status for users") 
    print("   📈 Trend Analysis: Health over time, velocity metrics")
    print("   🔄 CI/CD Integration: Automated status updates from builds")
    print("   🤖 Autonomous Sentinels: AI agents running test scripts 24/7")
    print("   💬 Usage Analytics: Track most/least used commands")
    print("   🎯 Smart Test Selection: Dependency-aware test prioritization")
    print("   📝 Git Workflow Integration: Auto-enhance commits with status")
    print("   🔗 Command Usage Graph: Visual dependency relationships")

async def show_broken():
    """Show just broken commands in dependency-ranked order"""
    print("🔴 BROKEN COMMANDS - DEPENDENCY-RANKED ORDER")
    print("=" * 50)
    
    tickets = get_command_tickets()
    broken_tickets = [t for t in tickets if '🔴' in t['status']]
    
    if broken_tickets:
        print(f"Found {len(broken_tickets)} broken commands (sorted by fix order):\n")
        
        # Sort by dependency order first (fix foundation commands first), then priority
        broken_tickets.sort(key=lambda x: (x['dep_order'], x['priority']))
        
        for i, ticket in enumerate(broken_tickets, 1):
            deps_str = f" → depends on: {', '.join(ticket['dependencies'])}" if ticket['dependencies'] else " → foundation command (no deps)"
            print(f"{i:2d}. 🔴 {ticket['name']}")
            print(f"     Issue: {ticket['issue']}")
            print(f"     Dependencies: {deps_str}")
            print(f"     TODOs: {ticket['todos']} | Last Updated: {ticket['date']}")
            print()
            
        print("🔧 LOGICAL FIX STRATEGY:")
        print("   1. Fix foundation commands first (no dependencies)")
        print("   2. Then fix commands that depend on the fixed ones")
        print("   3. This prevents cascade failures when testing")
        print()
    else:
        print("🎉 No broken commands found!")
    
    print("🔧 QUICK FIXES TO LOOK FOR:")
    print("   • 'Method signature mismatch' = Change instance to static method")
    print("   • 'Missing execute method' = Add execute method to class")
    print("   • 'Parameter parsing error' = Check server-side validation")

async def show_recent():
    """Show recent work"""
    print("📝 RECENT WORK - WHAT OTHERS DID")
    print("=" * 35)
    
    success, output, _ = await run_command("--cmd agents")
    if success:
        lines = output.split('\n')
        in_recent_section = False
        found_recent = False
        
        for line in lines:
            if 'WHAT THE LAST AGENT WORKED ON' in line:
                in_recent_section = True
                print(line.strip())
                continue
            elif in_recent_section and line.strip().startswith('•'):
                print(line.strip())
                found_recent = True
            elif in_recent_section and line.strip() == '':
                break
                
        if not found_recent:
            print("📋 No recent work recorded.")
    
    print("\n💭 LEARNING FROM OTHERS:")
    print("   Check README files for detailed learning notes")
    print("   Look for 'Investigation Results' sections")

async def show_quick():
    """Show just essential status"""
    print("⚡ QUICK STATUS")
    print("=" * 15)
    
    success, output, _ = await run_command("--cmd agents")
    if success:
        lines = output.split('\n')
        for line in lines:
            if 'PROJECT STATUS:' in line:
                status = line.split('PROJECT STATUS:')[1].strip()
                print(f"📊 {status}")
                break
    
    print("\n🧪 CRITICAL FIRST STEP:")
    print("   python3 ai-portal.py --cmd tests  # Run all tests")
    
    print("\n🎯 NEXT STEPS:")
    print("   python3 ai-agent.py           # Full dashboard")
    print("   python3 ai-agent.py --broken  # See broken commands")

async def run_tests():
    """Run Continuum tests and show results"""
    print("🧪 RUNNING CONTINUUM TEST SUITE")
    print("=" * 35)
    print("📡 Using multiple test approaches:")
    print("   1. Built-in diagnostics: continuum --test")
    print("   2. Dashboard unit tests: pytest tests/test_ai_dashboard.py")
    print()
    
    # Try diagnostics first
    print("🔬 Running Continuum diagnostics...")
    success, output, error = await run_command("--cmd diagnostics")
    
    if success:
        print("✅ DIAGNOSTICS COMPLETED")
        if output.strip():
            print("\n📄 Diagnostics Output:")
            print(output[:500] + "..." if len(output) > 500 else output)
    else:
        print("❌ DIAGNOSTICS FAILED")
        if error:
            print(f"\n🚨 Error: {error[:200]}...")
    
    # Also try the test command if it exists
    print("\n🧪 Checking test command...")
    test_success, test_output, test_error = await run_command("--cmd test")
    
    if test_success:
        print("✅ TEST COMMAND AVAILABLE")
    else:
        print("⚠️ Test command not available - using diagnostics only")
    
    overall_success = success or test_success
    print(f"\n💡 Result: {'System diagnostics passed' if overall_success else 'Issues detected - check broken commands'}")
    print("🔄 Next steps:")
    print("   python3 ai-agent.py --broken  # See what needs fixing")
    print("   continuum --test              # Run full system diagnostics")
    
    # Show quick health summary
    broken_count = 0
    try:
        # Quick check of broken commands
        import sys
        from pathlib import Path
        sys.path.insert(0, str(Path(__file__).parent))
        deps = get_command_dependencies() 
        tickets = get_command_tickets()
        broken_count = len([t for t in tickets if '🔴' in t['status']])
    except:
        pass
    
    print(f"\n📊 Current Status: {broken_count} broken commands need attention")

async def show_files_structure():
    """Show codebase structure and reduction plans"""
    files_path = Path(__file__).parent.parent / 'FILES.md'
    
    print("📁 CODEBASE STRUCTURE & REDUCTION MISSION")
    print("=" * 50)
    
    if files_path.exists():
        try:
            content = files_path.read_text()
            # Show just the header and goals section
            lines = content.split('\n')
            in_goals = False
            for line in lines[:50]:  # First 50 lines for overview
                if "## 🎯 Structure Goals" in line:
                    in_goals = True
                if "## 📋 File Tree" in line:
                    break
                if in_goals or line.startswith('# ') or line.startswith('> '):
                    print(line)
            
            print("\n🔧 STRUCTURE REDUCTION ACTIONS:")
            print("   1. Comment on every file in FILES.md - what it does, why it exists")
            print("   2. Identify consolidation opportunities (similar functionality)")
            print("   3. Flag files for deletion (dead code, redundant)")
            print("   4. Flatten deep directory structures")
            print("   5. Rename unclear files to be self-documenting")
            
            print(f"\n📊 Current metrics available in: {files_path}")
            print("   Run: ./scripts/generate-files-tree.sh  # To refresh structure")
            
        except Exception as e:
            print(f"❌ Error reading FILES.md: {e}")
    else:
        print("📋 FILES.md not found. Generate with:")
        print("   ./scripts/generate-files-tree.sh")
        print("\n🎯 STRUCTURE MISSION:")
        print("   Every file should be commented and justified.")
        print("   Goal: Reduce complexity, improve organization.")
        print("   Focus: Comment on leaves first, then consolidate.")

async def show_roadmap():
    """Show development roadmap loaded from ROADMAP.md"""
    roadmap_path = Path(__file__).parent.parent / 'ROADMAP.md'
    
    if roadmap_path.exists():
        try:
            content = roadmap_path.read_text()
            print(content)
        except Exception as e:
            print(f"❌ Error reading roadmap: {e}")
            print(f"📍 Roadmap location: {roadmap_path}")
    else:
        print("📋 ROADMAP NOT FOUND")
        print("=" * 25)
        print(f"📍 Expected location: {roadmap_path}")
        print("\n💡 To view the roadmap:")
        print("   1. Check if ROADMAP.md exists in project root")
        print("   2. Create roadmap file if missing")
        print("   3. Edit roadmap directly to refine priorities")
        
    print(f"\n🔄 ROADMAP EDITING:")
    print(f"   📝 Edit directly: {roadmap_path}")
    print(f"   💾 Commit changes: git add ROADMAP.md && git commit")
    print(f"   🔄 View updates: python3 ai-portal.py --roadmap")

async def show_logs():
    """Show debugging logs interface"""
    print("🛡️ SENTINEL DEBUGGING LOGS")
    print("=" * 30)
    
    # Get available sentinel tasks
    sentinel_dir = Path(__file__).parent.parent / '.continuum' / 'sentinel'
    
    if not sentinel_dir.exists():
        print("📂 No sentinel logs found. Start monitoring with:")
        print("   python3 ai-portal.py --cmd sentinel start debug-[task-name]")
        return
    
    tasks = [d for d in sentinel_dir.iterdir() if d.is_dir()]
    
    if not tasks:
        print("📂 No sentinel tasks found. Start monitoring with:")
        print("   python3 ai-portal.py --cmd sentinel start debug-[task-name]")
        return
    
    print(f"📋 Available debugging sessions ({len(tasks)} tasks):\n")
    
    # Sort by modification time (most recent first)
    tasks.sort(key=lambda x: x.stat().st_mtime, reverse=True)
    
    for i, task_dir in enumerate(tasks[:10], 1):  # Show last 10 tasks
        task_name = task_dir.name
        log_files = list(task_dir.glob("*.log"))
        
        if log_files:
            latest_log = max(log_files, key=lambda x: x.stat().st_mtime)
            mod_time = latest_log.stat().st_mtime
            import datetime
            mod_date = datetime.datetime.fromtimestamp(mod_time).strftime("%Y-%m-%d %H:%M")
            
            print(f"{i:2d}. 🛡️ {task_name}")
            print(f"    📁 {task_dir}")
            print(f"    📅 Last activity: {mod_date}")
            print(f"    📄 Log files: {len(log_files)}")
            
            # Show available log types
            log_types = []
            for log_file in log_files:
                if 'client-monitor' in log_file.name:
                    log_types.append('client')
                elif 'server-monitor' in log_file.name:
                    log_types.append('server')
                elif 'issues' in log_file.name:
                    log_types.append('issues')
                elif 'sentinel' in log_file.name:
                    log_types.append('sentinel')
            
            print(f"    🔍 Available: {', '.join(log_types)}")
            print()
    
    print("🔧 HOW TO VIEW LOGS:")
    print("   # View specific log type")
    print("   cat .continuum/sentinel/[task-name]/issues-*.log")
    print("   cat .continuum/sentinel/[task-name]/server-monitor-*.log")
    print("   cat .continuum/sentinel/[task-name]/client-monitor-*.log")
    print("   cat .continuum/sentinel/[task-name]/sentinel-*.log")
    print()
    print("   # View latest issues (most common)")
    print("   tail -20 .continuum/sentinel/*/issues-*.log")
    print()
    print("   # View all logs from specific task")
    print("   ls -la .continuum/sentinel/[task-name]/")
    
    print("\n📊 LOG CONTENTS:")
    print("   • 🔥 issues-*.log: Errors, warnings, console messages")
    print("   • 🖥️ server-monitor-*.log: Server-side execution logs")
    print("   • 🌐 client-monitor-*.log: Browser console logs & errors")
    print("   • 🛡️ sentinel-*.log: Sentinel bot activity & monitoring")
    
    print("\n💡 TIP: Start with issues-*.log for debugging - it has console errors!")

def main():
    """Main entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description='AI Agent Dashboard - Your Command Center')
    parser.add_argument('--dashboard', action='store_true', help='Show full dashboard')
    parser.add_argument('--broken', action='store_true', help='Show only broken commands')
    parser.add_argument('--recent', action='store_true', help='Show only recent work')
    parser.add_argument('--logs', action='store_true', help='Show debugging logs')
    parser.add_argument('--quick', action='store_true', help='Show quick status only')
    parser.add_argument('--test', action='store_true', help='Run Continuum test suite')
    parser.add_argument('--roadmap', action='store_true', help='Show development roadmap and big picture vision')
    parser.add_argument('--files', action='store_true', help='Show codebase structure and reduction mission')
    parser.add_argument('--sort', choices=['priority', 'date', 'name'], default='priority', 
                       help='Sort tickets by priority, date, or name')
    parser.add_argument('--filter', choices=['all', 'broken', 'testing', 'stable', 'untested'], default='all',
                       help='Filter by status type')
    parser.add_argument('--limit', type=int, default=10, help='Limit number of items shown')
    
    args = parser.parse_args()
    
    async def run():
        if args.test:
            await run_tests()
        elif args.roadmap:
            await show_roadmap()
        elif args.files:
            await show_files_structure()
        elif args.broken:
            await show_broken()
        elif args.recent:
            await show_recent()
        elif args.logs:
            await show_logs()
        elif args.quick:
            await show_quick()
        else:
            await show_dashboard()  # Default to dashboard (--dashboard flag or no args)
    
    asyncio.run(run())

if __name__ == "__main__":
    main()