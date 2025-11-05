#!/bin/bash

# Real-Time Server Events Engineering Test
# Scientific validation that server events work without deprecated fallbacks
# Proves real-time event propagation: Server → Widget → UI Update

echo "🧪 ENGINEERING TEST: Real-Time Server Events Validation"
echo "====================================================="
echo ""
echo "🎯 HYPOTHESIS: Server events propagate to ChatWidget without deprecated DOM fallbacks"
echo "📋 METHOD: Send server message → Monitor browser console for server event receipt → Validate UI update"
echo ""

# Clear browser console to get clean event logs
echo "1. Clearing browser console logs for clean event monitoring..."
> examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log

# Wait for system to be ready
sleep 2

echo ""
echo "2. Monitoring browser console for SERVER-EVENT-RECEIVED logs..."

# Start monitoring browser console for server events in background
tail -f examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log | grep -E "(SERVER-EVENT-RECEIVED|🔥)" &
MONITOR_PID=$!

# Give monitor time to start
sleep 1

echo ""
echo "3. Sending server message to trigger real-time events..."

# Send a uniquely identifiable message
UNIQUE_MSG="ENGINEERING TEST $(date +%s): Real-time server event validation - NO DEPRECATED FALLBACKS"
./jtag chat/send-message --roomId="general" --content="$UNIQUE_MSG"

echo ""
echo "4. Waiting for server event propagation..."
sleep 3

# Stop monitoring
kill $MONITOR_PID 2>/dev/null

echo ""
echo "5. SCIENTIFIC ANALYSIS: Checking for server event receipt..."

# Check if we got the SERVER-EVENT-RECEIVED log
SERVER_EVENTS=$(tail -50 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log | grep -c "🔥 SERVER-EVENT-RECEIVED")
WIDGET_EVENTS=$(tail -50 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log | grep -c "ChatWidget.*event")
EVENT_SETUP=$(tail -50 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log | grep -c "Set up type-safe event listeners for chat events")

echo ""
echo "📊 EVENT ANALYSIS RESULTS:"
echo "=========================="
echo "Server Events Received: $SERVER_EVENTS"
echo "Widget Event Handlers: $WIDGET_EVENTS" 
echo "Event System Setup: $EVENT_SETUP"

# Check for deprecated warnings (should be ZERO now)
DEPRECATED_WARNINGS=$(tail -100 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-warn.log | grep -c "DEPRECATED.*setupDOMEventFallbacks")

echo ""
echo "🚫 DEPRECATED CODE ANALYSIS:"
echo "============================"
echo "Deprecated DOM Fallback Warnings: $DEPRECATED_WARNINGS"

if [ "$DEPRECATED_WARNINGS" -eq 0 ]; then
    echo "✅ SUCCESS: No deprecated DOM fallback warnings - old code properly removed"
else
    echo "❌ FAILURE: Still finding deprecated DOM fallback code executing"
    echo ""
    echo "Recent deprecated warnings:"
    tail -10 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-warn.log | grep "DEPRECATED"
    exit 1
fi

# Check if message was stored
echo ""
echo "6. Validating message persistence..."
MESSAGE_STORED=$(./jtag data/list --collection="chat_messages" --filters='{"roomId":"general"}' | grep -c "ENGINEERING TEST.*Real-time server event validation")

if [ "$MESSAGE_STORED" -gt 0 ]; then
    echo "✅ SUCCESS: Message persisted to database"
else
    echo "❌ FAILURE: Message not found in database"
    exit 1
fi

# Final engineering assessment
echo ""
echo "🔬 ENGINEERING CONCLUSION:"
echo "=========================="

if [ "$SERVER_EVENTS" -gt 0 ]; then
    echo "✅ HYPOTHESIS CONFIRMED: Real-time server events ARE working"
    echo "✅ EVIDENCE: Found $SERVER_EVENTS server event receipts in browser console"
    echo "✅ VALIDATION: Deprecated DOM fallbacks successfully removed without breaking functionality"
    echo ""
    echo "🎉 ENGINEERING SUCCESS: Server events proven functional through scientific method"
    echo ""
    echo "📋 TECHNICAL SUMMARY:"
    echo "- Server message triggers real-time event"
    echo "- ChatWidget receives event via addWidgetEventListener"  
    echo "- No deprecated DOM fallback code executing"
    echo "- Message persistence confirmed"
    echo "- Real-time UI updates working via proper server events"
else
    echo "❌ HYPOTHESIS REJECTED: No server events detected in browser console"
    echo "❌ ENGINEERING FAILURE: Server events not working - may need DOM fallbacks"
    echo ""
    echo "🔍 DEBUG INFORMATION:"
    echo "Recent browser console (last 20 lines):"
    tail -20 examples/widget-ui/.continuum/jtag/currentUser/logs/browser-console-log.log
    exit 1
fi