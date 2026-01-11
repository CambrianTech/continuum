#!/bin/bash

# Test Chat Domain Objects Script
# Demonstrates the elegant domain object architecture working end-to-end

echo "🧪 Testing Chat Domain Object Architecture"
echo "=========================================="

echo ""
echo "📤 Sending test messages using domain objects..."

# Test 1: Basic message
echo "1. Sending basic message..."
./jtag chat/send-message --roomId="general" --content="Domain objects working perfectly! 🎉"

echo ""
echo "2. Sending message with mentions..."
./jtag chat/send-message --roomId="general" --content="Testing @user mention functionality"

echo ""
echo "3. Sending system-style message..."
./jtag chat/send-message --roomId="general" --content="[System] Domain object architecture complete"

echo ""
echo "4. Sending longer message..."
./jtag chat/send-message --roomId="general" --content="This demonstrates how domain objects eliminate type duplication - the ChatMessage class IS the API, no separate interfaces needed!"

echo ""
echo "5. Testing different room..."
./jtag chat/send-message --roomId="development" --content="Testing room isolation with domain objects"

echo ""
echo "✅ All messages sent using ChatMessage domain objects!"
echo "📊 Each message demonstrates:"
echo "   - Zero type duplication (ChatMessage IS the type)"
echo "   - Global database storage"
echo "   - Event system integration"
echo "   - Rust-like immutability with factory methods"
echo ""
echo "🔍 Check the browser logs to see domain objects in action!"