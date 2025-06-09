#!/usr/bin/env python3
"""
Claude Unified Debugger - Entry Point
====================================

Clean entry point for the modular Claude debugger system.
"""

import asyncio
from claude_debugger.main import main

if __name__ == "__main__":
    asyncio.run(main())