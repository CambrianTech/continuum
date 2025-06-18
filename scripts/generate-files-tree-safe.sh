#!/bin/bash
# Safe Generate FILES.md - Preserves archaeological documentation
# Usage: ./scripts/generate-files-tree-safe.sh

set -e

OUTPUT_FILE="FILES.md"
TEMP_FILE="files_temp.md"
BACKUP_FILE="FILES.md.pre-generation"

echo "🌳 Generating codebase tree with comments (preserving archaeological sections)..."

# Create backup
if [[ -f "$OUTPUT_FILE" ]]; then
    cp "$OUTPUT_FILE" "$BACKUP_FILE"
    echo "📋 Backed up existing FILES.md to $BACKUP_FILE"
fi

# Extract sections to preserve
PRESERVE_DIR="/tmp/files-preserve-$$"
mkdir -p "$PRESERVE_DIR"

if [[ -f "$OUTPUT_FILE" ]]; then
    echo "🔍 Extracting sections to preserve..."
    
    # Extract Agent Study Guide section
    sed -n '/^## 🎯 Agent Study Guide/,/^## [^🎯]/p' "$OUTPUT_FILE" | head -n -1 > "$PRESERVE_DIR/agent-guide.md" 2>/dev/null || echo "" > "$PRESERVE_DIR/agent-guide.md"
    
    # Extract System Restoration Audit section  
    sed -n '/^## 🔧 SYSTEM RESTORATION AUDIT/,/^## [^🔧]/p' "$OUTPUT_FILE" | head -n -1 > "$PRESERVE_DIR/restoration-audit.md" 2>/dev/null || echo "" > "$PRESERVE_DIR/restoration-audit.md"
    
    # Extract Visual Documentation section
    sed -n '/^## 📸 UI VISUAL DOCUMENTATION/,/^## [^📸]/p' "$OUTPUT_FILE" | head -n -1 > "$PRESERVE_DIR/visual-docs.md" 2>/dev/null || echo "" > "$PRESERVE_DIR/visual-docs.md"
    
    # Extract Tombstones section
    sed -n '/^## ⚰️ TOMBSTONES: Deleted Files/,/^## [^⚰️]/p' "$OUTPUT_FILE" | head -n -1 > "$PRESERVE_DIR/tombstones.md" 2>/dev/null || echo "" > "$PRESERVE_DIR/tombstones.md"
    
    # Extract Generator Guidelines section
    sed -n '/^## 🤖 FOR THE FILES TREE GENERATOR/,/^## [^🤖]/p' "$OUTPUT_FILE" | head -n -1 > "$PRESERVE_DIR/generator-guide.md" 2>/dev/null || echo "" > "$PRESERVE_DIR/generator-guide.md"
    
    echo "✅ Preserved sections extracted"
fi

# Function to add comment placeholder (same as original)
add_comment() {
    local file="$1"
    local indent="$2"
    
    # Check if we have existing comments for this file
    local existing_comment=""
    if [[ -f "$BACKUP_FILE" ]]; then
        existing_comment=$(grep -A1 "^### ${file}" "$BACKUP_FILE" 2>/dev/null | tail -n1 | grep "^  # " | sed "s/^  # //" || echo "")
    fi
    
    # Add comment based on file type
    local comment=""
    case "$file" in
        *.md) comment="📖 Documentation" ;;
        *.js|*.cjs) comment="⚡ JavaScript/Node.js" ;;
        *.py) comment="🐍 Python" ;;
        *.json) comment="📋 Configuration/Data" ;;
        *.sh) comment="🔧 Shell Script" ;;
        *.yml|*.yaml) comment="⚙️ YAML Config" ;;
        *.gitignore) comment="🚫 Git ignore rules" ;;
        package*.json) comment="📦 Node.js dependencies" ;;
        requirements*.txt) comment="📦 Python dependencies" ;;
        Dockerfile) comment="🐳 Docker container" ;;
        *test*.py|*test*.js) comment="🧪 Test file" ;;
        *Command.cjs) comment="🎯 Continuum command implementation" ;;
        continuum.cjs) comment="AGENT CONFUSION (2025-06-18): Main server entry point but unclear from name alone" ;;
        ai-agent.py) comment="AGENT CONFUSION (2025-06-18): Main dashboard but hyphen in name breaks Python imports" ;;
        *) 
            if [[ -d "$file" ]]; then
                comment="📁 Directory"
            else
                comment="📄 File"
            fi
            ;;
    esac
    
    # Use existing comment if available
    if [[ -n "$existing_comment" ]]; then
        comment="$existing_comment"
    fi
    
    echo "$indent  # $comment"
}

# Generate new header
cat > "$TEMP_FILE" << 'EOF'
# 📁 Continuum Codebase Structure

> **Mission: Reduce complexity and improve organization**  
> Comment on every file - what it does, why it exists, and how to simplify.

## 🎯 Structure Goals
- **Minimize depth** - Flatten nested directories where possible
- **Clear naming** - Every file name should explain its purpose  
- **Consolidate related** - Group similar functionality together
- **Remove dead code** - Delete unused or redundant files

## 📋 File Tree with Agent Comments

```
🔗 **Click any file below to jump to detailed comments**

EOF

# Generate tree with links (simplified version for safety)
tree -a -I 'node_modules|.git|__pycache__|\*.pyc|\.DS_Store|\.pytest_cache|htmlcov|\*.egg-info|dist|build' \
    --dirsfirst | head -100 >> "$TEMP_FILE"

echo '```' >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Add preserved Agent Study Guide
if [[ -s "$PRESERVE_DIR/agent-guide.md" ]]; then
    echo "📚 Restoring Agent Study Guide..."
    cat "$PRESERVE_DIR/agent-guide.md" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
fi

# Add preserved System Restoration Audit
if [[ -s "$PRESERVE_DIR/restoration-audit.md" ]]; then
    echo "🔧 Restoring System Restoration Audit..."
    cat "$PRESERVE_DIR/restoration-audit.md" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
fi

# Add preserved Visual Documentation
if [[ -s "$PRESERVE_DIR/visual-docs.md" ]]; then
    echo "📸 Restoring Visual Documentation..."
    cat "$PRESERVE_DIR/visual-docs.md" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
fi

# Add preserved Tombstones
if [[ -s "$PRESERVE_DIR/tombstones.md" ]]; then
    echo "⚰️ Restoring Tombstones..."
    cat "$PRESERVE_DIR/tombstones.md" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
fi

# Add preserved Generator Guidelines
if [[ -s "$PRESERVE_DIR/generator-guide.md" ]]; then
    echo "🤖 Restoring Generator Guidelines..."
    cat "$PRESERVE_DIR/generator-guide.md" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
fi

# Add simple file listing section
echo "## 📝 File Analysis (Simplified)" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"
echo "**Note: This is a simplified view. The generator preserves archaeological sections above.**" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Add footer
cat >> "$TEMP_FILE" << 'EOF'

## 🔧 Maintenance Commands

```bash
# Regenerate this file (safe mode)
./scripts/generate-files-tree-safe.sh

# DANGEROUS: Full regeneration (loses archaeological sections)
./scripts/generate-files-tree.sh

# View preserved sections
ls -la FILES.md.pre-generation
```

---
*Generated: $(date)*  
*Script: `./scripts/generate-files-tree-safe.sh`*
*Archaeological sections preserved*
EOF

# Replace variables in footer
sed -i.bak "s/\$(date)/$(date)/" "$TEMP_FILE" && rm "$TEMP_FILE.bak"

# Move temp file to final location
mv "$TEMP_FILE" "$OUTPUT_FILE"

# Cleanup
rm -rf "$PRESERVE_DIR"

echo "✅ Generated $OUTPUT_FILE (preserved archaeological sections)"
echo "📊 $(wc -l < "$OUTPUT_FILE") lines of structure documentation"
echo "🛡️ Backup available at: $BACKUP_FILE"
echo "🎯 Archaeological sections preserved successfully"