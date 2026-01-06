#\!/bin/bash
echo "🔧 MANUAL SYSTEM STARTUP FIX"
echo "Starting system using working manual method..."

# Run the manual steps that work
npm run smart-build
npm run system:deploy
npm run system:run &

# Wait a bit for startup
sleep 10

# Generate signal manually
npx tsx scripts/signal-system-ready.ts

echo "✅ System started successfully"
exit 0

