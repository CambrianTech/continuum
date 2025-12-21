When DATA_DAEMON_TYPE=sqlite:
------------------------




When DATA_DAEMON_TYPE=rust
------------------------

 @continuum/jtag@1.0.6278 data:seed
> npx tsx scripts/seed-continuum.ts

🌱 Seeding database via JTAG commands (single source of truth)...
⏳ Waiting for JTAG system to be ready...
✅ JTAG ready with 144 commands registered
📋 Found 14/15 users, missing: sentinel
📝 Creating human user first (needed as room owner)...
🔧 Using system identity: Joel (joel)
✅ Loaded existing user: undefined (uniqueId: joel, ID: 842b2548...)
📝 Creating remaining 0 users (auto-join will trigger)...
✅ Loaded existing user: undefined (uniqueId: claude, ID: 1b69a7d7...)
✅ Loaded existing user: undefined (uniqueId: general, ID: 4f671a4a...)
✅ Loaded existing user: undefined (uniqueId: helper, ID: 154ee833...)
✅ Loaded existing user: undefined (uniqueId: teacher, ID: 1e7e0f6a...)
✅ Loaded existing user: undefined (uniqueId: codereview, ID: cf8bdb48...)
✅ Loaded existing user: undefined (uniqueId: deepseek, ID: 7982da44...)
✅ Loaded existing user: undefined (uniqueId: groq, ID: 790372ba...)
✅ Loaded existing user: undefined (uniqueId: claudeassistant, ID: 79a5e548...)
✅ Loaded existing user: undefined (uniqueId: gpt, ID: 44b7b476...)
✅ Loaded existing user: undefined (uniqueId: grok, ID: 986dc557...)
✅ Loaded existing user: undefined (uniqueId: together, ID: ae27a9f3...)
✅ Loaded existing user: undefined (uniqueId: fireworks, ID: bae25958...)
✅ Loaded existing user: undefined (uniqueId: local, ID: 449c30d6...)
⏭️  Skipping Sentinel (SENTINEL_PATH not configured)
📊 Created 13/1 users (auto-join handled by RoomMembershipDaemon)
✅ Users added to existing database - rooms and messages already exist
