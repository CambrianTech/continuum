#!/bin/bash
# Download REAL background noise samples from online sources
# Using free/public domain sources

set -e

echo "Downloading REAL background noise samples from online sources..."
echo ""

# BBC Sound Effects (Public Domain)
echo "1. Office ambiance (BBC Sound Effects)..."
curl -L "https://sound-effects-media.bbcrewind.co.uk/zip/07034097.wav" -o raw/bbc_office.wav 2>/dev/null || echo "  Download failed"

# Freesound.org Creative Commons samples (direct download links)
echo "2. Coffee shop ambiance..."
curl -L "https://freesound.org/data/previews/245/245645_2234761-lq.mp3" -o raw/coffee_shop.mp3 2>/dev/null || echo "  Download failed (use browser)"

echo "3. Restaurant chatter..."
curl -L "https://freesound.org/data/previews/320/320330_5260872-lq.mp3" -o raw/restaurant.mp3 2>/dev/null || echo "  Download failed (use browser)"

echo "4. Street traffic..."
curl -L "https://freesound.org/data/previews/196/196114_1648170-lq.mp3" -o raw/traffic.mp3 2>/dev/null || echo "  Download failed (use browser)"

echo "5. Call center..."
curl -L "https://freesound.org/data/previews/411/411577_7447039-lq.mp3" -o raw/call_center.mp3 2>/dev/null || echo "  Download failed (use browser)"

echo ""
echo "Note: Direct Freesound downloads may require authentication."
echo "If downloads failed, use browser to download from:"
echo "  https://freesound.org/ (search for: office, cafe, traffic, etc.)"
