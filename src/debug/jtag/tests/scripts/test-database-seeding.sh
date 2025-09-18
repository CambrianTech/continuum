#!/bin/bash

# Test Database Seeding Script
# Seeds realistic users and chat rooms into both JSON and SQLite backends

echo "🌱 Database Seeding - Users & Chat Rooms"
echo "========================================"

echo ""
echo "🎯 This script seeds consistent test data into JTAG:"
echo "   👥 Users: Joel, Claude Code, Assistant Alpha + test users"
echo "   🏠 Rooms: general, academy, development channels"
echo "   💬 Messages: Realistic conversation history"
echo "   🔗 Participations: User-room relationships"

echo ""
echo "📋 Current Data Status:"
echo "Checking existing users..."
./jtag data/list --collection=users --limit=3

echo ""
echo "Checking existing rooms..."
./jtag data/list --collection=rooms --limit=3

echo ""
echo "🌱 Starting seeding process..."

echo ""
echo "👥 SEEDING USERS"
echo "=================="

echo ""
echo "1. Creating Joel (Human User - Creator)..."
./jtag data/create --collection=users --data='{
  "id": "user-joel-12345",
  "name": "Joel",
  "displayName": "Joel - Creator",
  "userType": "human",
  "email": "joel@continuum.dev",
  "isOnline": true,
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActiveAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "avatar": "👨‍💻",
  "preferences": {
    "theme": "dark",
    "notifications": true,
    "autoComplete": true
  }
}'

echo ""
echo "2. Creating Claude Code (Agent User)..."
./jtag data/create --collection=users --data='{
  "id": "agent-claude-code-67890",
  "name": "Claude Code",
  "displayName": "Claude Code Agent",
  "userType": "agent",
  "isOnline": true,
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActiveAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "avatar": "🤖",
  "capabilities": ["code-generation", "debugging", "architecture", "testing"],
  "metadata": {
    "model": "claude-sonnet-4",
    "provider": "anthropic"
  }
}'

echo ""
echo "3. Creating Assistant Alpha (Persona User)..."
./jtag data/create --collection=users --data='{
  "id": "persona-alpha-11111",
  "name": "Assistant Alpha",
  "displayName": "Alpha Persona",
  "userType": "persona",
  "email": "alpha@continuum.ai",
  "isOnline": false,
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActiveAt": "'$(date -u -d "1 hour ago" +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "avatar": "🎭",
  "capabilities": ["natural-language", "user-assistance", "task-planning"]
}'

echo ""
echo "4. Creating Alice (Test Human User)..."
./jtag data/create --collection=users --data='{
  "id": "user-alice-22222",
  "name": "Alice",
  "displayName": "Alice Developer",
  "userType": "human",
  "email": "alice@test.dev",
  "isOnline": true,
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActiveAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "avatar": "👩‍💻",
  "preferences": {
    "theme": "light",
    "notifications": false
  }
}'

echo ""
echo "5. Creating DataBot (Agent User)..."
./jtag data/create --collection=users --data='{
  "id": "agent-databot-33333",
  "name": "DataBot",
  "displayName": "Data Analysis Agent",
  "userType": "agent",
  "isOnline": true,
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActiveAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "avatar": "📊",
  "capabilities": ["data-analysis", "sql-queries", "reporting"],
  "metadata": {
    "specialization": "database-optimization"
  }
}'

echo ""
echo "✅ Users seeded! Verifying..."
./jtag data/list --collection=users

echo ""
echo "🏠 SEEDING CHAT ROOMS"
echo "====================="

echo ""
echo "1. Creating 'general' room (Public)..."
./jtag data/create --collection=rooms --data='{
  "id": "room-general-11111",
  "name": "general",
  "displayName": "General Discussion",
  "type": "public",
  "description": "Main discussion room for all members",
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActivity": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "memberCount": 5,
  "isArchived": false,
  "metadata": {
    "purpose": "main-chat",
    "createdBy": "user-joel-12345"
  }
}'

echo ""
echo "2. Creating 'academy' room (Private)..."
./jtag data/create --collection=rooms --data='{
  "id": "room-academy-22222",
  "name": "academy",
  "displayName": "AI Academy",
  "type": "private",
  "description": "Private training and development discussions",
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActivity": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "memberCount": 3,
  "isArchived": false,
  "metadata": {
    "purpose": "training",
    "createdBy": "user-joel-12345"
  }
}'

echo ""
echo "3. Creating 'development' room (Public)..."
./jtag data/create --collection=rooms --data='{
  "id": "room-development-33333",
  "name": "development",
  "displayName": "Development Chat",
  "type": "public",
  "description": "Technical development discussions",
  "createdAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastActivity": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "memberCount": 4,
  "isArchived": false,
  "metadata": {
    "purpose": "development",
    "createdBy": "user-joel-12345"
  }
}'

echo ""
echo "✅ Rooms seeded! Verifying..."
./jtag data/list --collection=rooms

echo ""
echo "💬 SEEDING SAMPLE MESSAGES"
echo "========================="

echo ""
echo "1. Welcome message in general..."
./jtag data/create --collection=messages --data='{
  "id": "msg-welcome-11111",
  "roomId": "room-general-11111",
  "senderId": "user-joel-12345",
  "content": "Welcome to the general discussion room! 🎉",
  "type": "text",
  "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'"
}'

echo ""
echo "2. Agent response in general..."
./jtag data/create --collection=messages --data='{
  "id": "msg-agent-response-22222",
  "roomId": "room-general-11111",
  "senderId": "agent-claude-code-67890",
  "content": "Hello everyone! Ready to help with any coding questions! 🤖",
  "type": "text",
  "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'"
}'

echo ""
echo "3. Academy announcement..."
./jtag data/create --collection=messages --data='{
  "id": "msg-academy-announcement-33333",
  "roomId": "room-academy-22222",
  "senderId": "persona-alpha-11111",
  "content": "AI Academy training session starting soon! 🎓",
  "type": "text",
  "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'"
}'

echo ""
echo "✅ Sample messages seeded!"

echo ""
echo "🔗 SEEDING USER-ROOM PARTICIPATIONS"
echo "===================================="

echo ""
echo "1. Joel joins all rooms (as admin)..."
./jtag data/create --collection=participations --data='{
  "id": "participation-joel-general",
  "userId": "user-joel-12345",
  "roomId": "room-general-11111",
  "role": "admin",
  "joinedAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastReadAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "isActive": true,
  "notifications": true
}'

./jtag data/create --collection=participations --data='{
  "id": "participation-joel-academy",
  "userId": "user-joel-12345",
  "roomId": "room-academy-22222",
  "role": "admin",
  "joinedAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastReadAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "isActive": true,
  "notifications": true
}'

echo ""
echo "2. Claude Code joins general and development..."
./jtag data/create --collection=participations --data='{
  "id": "participation-claude-general",
  "userId": "agent-claude-code-67890",
  "roomId": "room-general-11111",
  "role": "moderator",
  "joinedAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastReadAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "isActive": true,
  "notifications": true
}'

./jtag data/create --collection=participations --data='{
  "id": "participation-claude-development",
  "userId": "agent-claude-code-67890",
  "roomId": "room-development-33333",
  "role": "moderator",
  "joinedAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastReadAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "isActive": true,
  "notifications": true
}'

echo ""
echo "3. Alice joins general room..."
./jtag data/create --collection=participations --data='{
  "id": "participation-alice-general",
  "userId": "user-alice-22222",
  "roomId": "room-general-11111",
  "role": "member",
  "joinedAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "lastReadAt": "'$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)'",
  "isActive": true,
  "notifications": false
}'

echo ""
echo "✅ Participations seeded!"

echo ""
echo "📊 FINAL DATA SUMMARY"
echo "====================="

echo ""
echo "Users in system:"
./jtag data/list --collection=users

echo ""
echo "Rooms in system:"
./jtag data/list --collection=rooms

echo ""
echo "Messages in system:"
./jtag data/list --collection=messages --limit=5

echo ""
echo "Participations in system:"
./jtag data/list --collection=participations --limit=5

echo ""
echo "🔍 Testing queries..."
echo ""
echo "Online users:"
./jtag data/list --collection=users --filter='{"isOnline":true}'

echo ""
echo "Agent users:"
./jtag data/list --collection=users --filter='{"userType":"agent"}'

echo ""
echo "Public rooms:"
./jtag data/list --collection=rooms --filter='{"type":"public"}'

echo ""
echo "✅ Database seeding complete!"
echo ""
echo "🎯 What was seeded:"
echo "   👥 5 Users: 2 humans, 2 agents, 1 persona"
echo "   🏠 3 Rooms: general (public), academy (private), development (public)"
echo "   💬 3 Messages: Welcome messages and announcements"
echo "   🔗 5 Participations: User-room relationships with roles"
echo ""
echo "🧪 Ready for testing:"
echo "   - Chat functionality with realistic users and rooms"
echo "   - User authentication and permissions"
echo "   - Message history and threading"
echo "   - Room membership and moderation"
echo "   - Agent interactions and capabilities"