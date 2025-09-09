#!/bin/bash

# Test All Themes - Shell Script Version
# Gets themes via list command (not hardcoded) and screenshots each one

echo "🎨 AUTOMATED THEME TESTING"
echo "========================="

# Get theme list dynamically
echo "📋 Getting theme list..."
THEME_OUTPUT=$(./jtag theme/list)
echo "$THEME_OUTPUT"

# Extract just the theme names (base, classic, cyberpunk, light, monochrome, retro-mac)
THEMES=$(echo "$THEME_OUTPUT" | grep -A 20 '"themes"' | grep -o '"[^"]*"' | grep -v themes | tr -d '"' | head -6)

echo ""
echo "🎨 Found themes:"
for theme in $THEMES; do
    echo "  - $theme"
done

echo ""
echo "📸 Starting theme screenshot test..."

# Iterate through each theme
for theme in $THEMES; do
    echo ""
    echo "🔄 Testing theme: $theme"
    echo "-------------------------"
    
    # Set theme
    echo "🎨 Setting theme to $theme..."
    ./jtag theme/set "$theme"
    
    # Give time for visual changes
    sleep 2
    
    # Take screenshot
    echo "📸 Taking screenshot..."
    ./jtag screenshot --querySelector=body --filename="automated-theme-$theme.png"
    
    echo "✅ Completed: $theme"
done

echo ""
echo "🎉 THEME TEST COMPLETE!"
echo "======================"
echo "📁 All screenshots saved to same session directory"
echo "🔍 Check .continuum/sessions/user/*/screenshots/ for results"