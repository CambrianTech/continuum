#!/usr/bin/env python3
"""
Quick test of console reading and code execution
"""

import asyncio
from claude_debugger.connection import WebSocketConnection
from claude_debugger.validation import JavaScriptValidator
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

async def test_console_and_execution():
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    
    connection = WebSocketConnection(ws_url)
    await connection.connect()
    
    # Skip banner
    await connection.receive_message()
    await connection.receive_message()
    
    js_validator = JavaScriptValidator(connection)
    
    # Test 1: Read current page info
    print("🔍 TEST 1: Reading page information...")
    result1 = await js_validator.execute_and_wait('''
        console.log("🔍 Claude reading page info");
        console.log("URL:", window.location.href);
        console.log("Title:", document.title);
        console.log("Version badge:", document.querySelector(".version-badge")?.textContent);
        return "PAGE_INFO_READ";
    ''')
    print(f"Result 1: {result1}")
    
    # Test 2: Execute diagnostic code
    print("\n🧪 TEST 2: Running diagnostics...")
    result2 = await js_validator.execute_and_wait('''
        console.log("🧪 Claude running diagnostics");
        const errors = [];
        const warnings = [];
        
        // Check for common issues
        if (!document.querySelector(".version-badge")) {
            errors.push("No version badge found");
        }
        if (typeof html2canvas === "undefined") {
            warnings.push("html2canvas not available");
        }
        
        console.log("Errors found:", errors.length);
        console.log("Warnings found:", warnings.length);
        
        return JSON.stringify({
            errors: errors,
            warnings: warnings,
            status: errors.length === 0 ? "HEALTHY" : "ISSUES_FOUND"
        });
    ''')
    print(f"Result 2: {result2}")
    
    await connection.disconnect()
    print("✅ Console reading and code execution confirmed!")

if __name__ == "__main__":
    asyncio.run(test_console_and_execution())