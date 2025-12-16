#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}🔨 Building Rust workers...${NC}"

# Build LoggerWorker
cd workers/logger
cargo build --release
cd ../..

# Build TrainingWorker
cd workers/training
cargo build --release
cd ../..

echo -e "${GREEN}✅ Build complete${NC}"

# Setup log directory
mkdir -p .continuum/jtag/logs/system

# Kill existing workers
echo -e "${YELLOW}🔄 Stopping existing workers...${NC}"
pkill -f logger-worker || true
pkill -f training-worker || true

# Give processes time to die and release sockets (macOS needs more time)
sleep 1.5

# Remove old sockets
rm -f /tmp/jtag-logger-worker.sock
rm -f /tmp/training-worker.sock

# Extra safety: wait for sockets to be fully removed
sleep 0.5

# Start LoggerWorker
echo -e "${YELLOW}🚀 Starting LoggerWorker...${NC}"
workers/logger/target/release/logger-worker /tmp/jtag-logger-worker.sock \
  > .continuum/jtag/logs/system/rust-worker.log 2>&1 &
LOGGER_PID=$!

# Wait for LoggerWorker socket to be created
for i in {1..10}; do
  if [ -S /tmp/jtag-logger-worker.sock ]; then
    echo -e "${GREEN}✅ LoggerWorker started (PID: $LOGGER_PID)${NC}"
    break
  fi
  if [ $i -eq 10 ]; then
    echo -e "${RED}❌ LoggerWorker failed to start${NC}"
    exit 1
  fi
  sleep 0.5
done

# Start TrainingWorker
echo -e "${YELLOW}🚀 Starting TrainingWorker...${NC}"
workers/training/target/release/training-worker /tmp/training-worker.sock \
  >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
TRAINING_PID=$!

# Wait for TrainingWorker socket to be created
for i in {1..10}; do
  if [ -S /tmp/training-worker.sock ]; then
    echo -e "${GREEN}✅ TrainingWorker started (PID: $TRAINING_PID)${NC}"
    break
  fi
  if [ $i -eq 10 ]; then
    echo -e "${RED}❌ TrainingWorker failed to start${NC}"
    exit 1
  fi
  sleep 0.5
done

# Verify all workers are running
sleep 0.5
if pgrep -f logger-worker > /dev/null && pgrep -f training-worker > /dev/null; then
  echo -e "${GREEN}✅ All workers running successfully${NC}"
  echo -e "   LoggerWorker:   PID $LOGGER_PID (/tmp/jtag-logger-worker.sock)"
  echo -e "   TrainingWorker: PID $TRAINING_PID (/tmp/training-worker.sock)"
  exit 0
else
  echo -e "${RED}❌ One or more workers failed to start${NC}"
  exit 1
fi
