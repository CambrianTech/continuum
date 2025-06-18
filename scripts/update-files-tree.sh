#!/bin/bash
# Update FILES.md tree section gracefully
# Usage: ./scripts/update-files-tree.sh

set -e

OUTPUT_FILE="FILES.md"
TEMP_TREE="tree_temp.txt"

echo "🌳 Updating file tree section gracefully..."

# Generate just the tree part
echo "Generating fresh tree..."
tree -a -I 'node_modules|.git|__pycache__|\*.pyc|\.DS_Store|\.pytest_cache|htmlcov|\*.egg-info|dist|build' \
    --dirsfirst | head -200 > "$TEMP_TREE"

# Check if FILES.md exists
if [[ -f "$OUTPUT_FILE" ]]; then
    echo "📋 FILES.md exists - updating tree section only..."
    
    # Create backup
    cp "$OUTPUT_FILE" "${OUTPUT_FILE}.backup"
    
    # Find the tree section and replace just that part
    if grep -q "^## 📋 File Tree with Agent Comments" "$OUTPUT_FILE"; then
        # Replace only the tree section between the markers
        awk '
        BEGIN { in_tree_section = 0; in_code_block = 0 }
        /^## 📋 File Tree with Agent Comments/ { 
            in_tree_section = 1
            print $0
            print ""
            print "```"
            print "🔗 **Click any file below to jump to detailed comments**"
            print ""
            # Insert new tree content
            system("cat '"$TEMP_TREE"'")
            print "```"
            print ""
            next
        }
        in_tree_section && /^```/ { 
            if (in_code_block) {
                in_tree_section = 0
                in_code_block = 0
                next
            } else {
                in_code_block = 1
                next
            }
        }
        in_tree_section && in_code_block { next }
        in_tree_section && /^##/ && !/^## 📋/ { 
            in_tree_section = 0
            print $0
            next
        }
        !in_tree_section { print $0 }
        ' "$OUTPUT_FILE" > "${OUTPUT_FILE}.new"
        
        mv "${OUTPUT_FILE}.new" "$OUTPUT_FILE"
        echo "✅ Updated tree section while preserving all other content"
    else
        echo "⚠️ Tree section not found - appending basic tree"
        echo "" >> "$OUTPUT_FILE"
        echo "## 📋 File Tree with Agent Comments" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        echo '```' >> "$OUTPUT_FILE"
        cat "$TEMP_TREE" >> "$OUTPUT_FILE"
        echo '```' >> "$OUTPUT_FILE"
    fi
else
    echo "📄 Creating new FILES.md..."
    cat > "$OUTPUT_FILE" << 'EOF'
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
    cat "$TEMP_TREE" >> "$OUTPUT_FILE"
    echo '```' >> "$OUTPUT_FILE"
fi

# Cleanup
rm "$TEMP_TREE"

echo "✅ Updated $OUTPUT_FILE gracefully"
echo "📊 $(wc -l < "$OUTPUT_FILE") lines total"
echo "🛡️ Backup: ${OUTPUT_FILE}.backup"