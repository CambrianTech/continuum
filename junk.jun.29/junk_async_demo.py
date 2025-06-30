#!/usr/bin/env python3
"""
Elegant Async Continuum Python Client Demo
Shows clean async/await patterns with modular architecture
"""

import asyncio
from continuum_client.async_client import connect, EventType, LogLevel

async def main():
    print("🐍 Elegant Async Continuum Client Demo")
    print("=" * 45)
    
    # Single connect() - clean and elegant
    async with await connect() as continuum:
        
        print("\n🔍 Dynamic Command Discovery:")
        commands = await continuum.list_commands()
        print(f"  • Found {len(commands)} commands")
        
        # Elegant async command execution
        print("\n⚡ Async Command Execution:")
        try:
            health = await continuum.health()
            print(f"  ✅ Health: {health}")
        except Exception as e:
            print(f"  ❌ Health failed: {e}")
        
        try:
            status = await continuum.daemon_status()
            print(f"  ✅ Daemons: {len(status.get('daemons', {}))} running")
        except Exception as e:
            print(f"  ❌ Daemon status failed: {e}")
        
        # JTAG hooks with async/await
        print("\n🎯 JTAG Autonomous Development Hooks:")
        try:
            screenshot = await continuum.browser_screenshot()
            if screenshot:
                print(f"  ✅ Screenshot: {len(screenshot)} bytes captured")
            else:
                print("  ⚠️ No screenshot available")
        except Exception as e:
            print(f"  ❌ Screenshot failed: {e}")
        
        try:
            logs = await continuum.console_logs(limit=5)
            print(f"  ✅ Console logs: {len(logs)} recent entries")
        except Exception as e:
            print(f"  ❌ Console logs failed: {e}")
        
        # Event streaming (most elegant feature)
        print("\n🌊 Real-time Event Streaming:")
        
        # Stream console logs for 3 seconds
        print("  📡 Streaming console logs...")
        timeout_task = asyncio.create_task(asyncio.sleep(3))
        stream_task = asyncio.create_task(stream_console_logs(continuum))
        
        done, pending = await asyncio.wait(
            [timeout_task, stream_task],
            return_when=asyncio.FIRST_COMPLETED
        )
        
        # Cancel pending tasks
        for task in pending:
            task.cancel()
        
        print("\n🏗️ Modular Architecture Benefits:")
        print("  • Clean separation: client.py, discovery.py, hooks.py, events.py")
        print("  • Type safety: Full TypeScript-style type hints")
        print("  • Async throughout: Native Python async/await patterns")
        print("  • Context managers: Automatic resource cleanup")
        print("  • Event-driven: Real-time streams for autonomous development")
        
        print("\n✨ Usage Patterns:")
        print("  # Context manager (recommended)")
        print("  async with await connect() as continuum:")
        print("      health = await continuum.health()")
        print("")
        print("  # Event streaming")
        print("  async for log in continuum.console_stream():")
        print("      print(f'Console: {log.data}')")
        print("")
        print("  # JTAG autonomous development")
        print("  screenshot = await continuum.browser_screenshot()")
        print("  await continuum.daemon_restart('renderer')")

async def stream_console_logs(continuum):
    """Demo real-time console log streaming"""
    count = 0
    try:
        async for log in continuum.console_stream():
            count += 1
            level_emoji = {
                'info': '💡',
                'warn': '⚠️',
                'error': '❌',
                'debug': '🔍'
            }.get(log.level, '📝')
            
            print(f"    {level_emoji} [{log.source}] {log.data.get('message', '')[:50]}...")
            
            if count >= 5:  # Limit for demo
                break
                
    except Exception as e:
        print(f"    ⚠️ Stream ended: {e}")
    
    print(f"  ✅ Streamed {count} console events")

async def advanced_event_demo():
    """Advanced event processing demo"""
    print("\n🔥 Advanced Event Processing:")
    
    async with await connect() as continuum:
        # Multi-stream filtering
        event_types = {EventType.CONSOLE, EventType.DAEMON}
        levels = {LogLevel.ERROR, LogLevel.WARN}
        
        print("  📊 Filtering: console + daemon events, errors + warnings only")
        
        count = 0
        timeout = asyncio.create_task(asyncio.sleep(2))
        
        async for event in continuum._events.filtered_stream(
            event_types=event_types,
            levels=levels
        ):
            print(f"    🚨 {event.type} {event.level}: {event.source}")
            count += 1
            
            if timeout.done() or count >= 3:
                break
        
        print(f"  ✅ Processed {count} filtered events")

if __name__ == "__main__":
    # Run main demo
    asyncio.run(main())
    
    # Run advanced demo if user wants
    print("\n" + "="*45)
    response = input("Run advanced event demo? (y/N): ")
    if response.lower().startswith('y'):
        asyncio.run(advanced_event_demo())