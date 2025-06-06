console.log("📋 Collecting all console warnings and errors...");
console.log("🔍 Current console state:");
console.log("- WebSocket status:", ws ? ws.readyState : "undefined");
console.log("- Connection attempts:", window.reconnectAttempts || 0);
console.log("- Last error log:", window.lastErrorLog || "none");
console.log("- Any visible warnings or errors should appear below this message");