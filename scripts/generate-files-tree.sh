#!/bin/bash
# Generate FILES.md - Living documentation of codebase structure
# Usage: ./scripts/generate-files-tree.sh

set -e

OUTPUT_FILE="FILES.md"
TEMP_FILE="files_temp.md"

echo "🌳 Generating codebase tree with comments..."

# Function to add comment placeholder
add_comment() {
    local file="$1"
    local indent="$2"
    
    # Check if we have existing comments for this file
    local existing_comment=""
    if [[ -f "$OUTPUT_FILE" ]]; then
        existing_comment=$(grep -A1 "^$indent$file" "$OUTPUT_FILE" 2>/dev/null | tail -n1 | grep "^$indent  # " | sed "s/^$indent  # //" || echo "")
    fi
    
    # Add comment based on file type and existing knowledge
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
        *) 
            if [[ -d "$file" ]]; then
                comment="📁 Directory"
            else
                comment="📄 File"
            fi
            ;;
    esac
    
    # Use existing comment if available, otherwise use default
    if [[ -n "$existing_comment" ]]; then
        comment="$existing_comment"
    fi
    
    echo "$indent  # $comment"
}

# Generate header
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
EOF

# Generate tree structure with filtering
tree -a -I 'node_modules|.git|__pycache__|\*.pyc|\.DS_Store|\.pytest_cache|htmlcov|\*.egg-info|dist|build' \
    --dirsfirst >> "$TEMP_FILE"

echo '```' >> "$TEMP_FILE"

# Add detailed file-by-file breakdown with comments
echo "" >> "$TEMP_FILE"
echo "## 📝 Detailed File Analysis" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Function to process directory recursively
process_directory() {
    local dir="$1"
    local indent="$2"
    
    echo "${indent}📁 **$dir/**" >> "$TEMP_FILE"
    
    # Process files in this directory
    find "$dir" -maxdepth 1 -type f ! -name '.*' | sort | while read -r file; do
        filename=$(basename "$file")
        echo "${indent}  - $filename" >> "$TEMP_FILE"
        add_comment "$filename" "$indent" >> "$TEMP_FILE"
    done
    
    # Process subdirectories
    find "$dir" -maxdepth 1 -type d ! -name '.*' ! -path "$dir" | sort | while read -r subdir; do
        if [[ ! "$subdir" =~ (node_modules|\.git|__pycache__|\.pytest_cache|htmlcov|dist|build) ]]; then
            dirname=$(basename "$subdir")
            process_directory "$subdir" "$indent  "
        fi
    done
}

# Start from root
echo "### Root Directory" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Process top-level files
find . -maxdepth 1 -type f ! -name '.*' | sort | while read -r file; do
    filename=$(basename "$file")
    echo "- $filename" >> "$TEMP_FILE"
    add_comment "$filename" "" >> "$TEMP_FILE"
done

echo "" >> "$TEMP_FILE"

# Process top-level directories
find . -maxdepth 1 -type d ! -name '.*' ! -path '.' | sort | while read -r dir; do
    if [[ ! "$dir" =~ (\./node_modules|\./\.git|\./\__pycache__|\./\.pytest_cache|\./htmlcov|\./dist|\./build) ]]; then
        dirname=${dir#./}
        process_directory "$dirname" ""
        echo "" >> "$TEMP_FILE"
    fi
done

# Add footer with commands
cat >> "$TEMP_FILE" << 'EOF'

## 🔧 Maintenance Commands

```bash
# Regenerate this file
./scripts/generate-files-tree.sh

# Add to dashboard sync
python3 python-client/ai-portal.py --cmd docs

# Find files that might be consolidatable
find . -name "*.py" -o -name "*.js" | grep -E "(util|helper|common)" 

# Find potential dead code
find . -name "*.py" -o -name "*.js" | xargs grep -l "TODO.*remove\|FIXME.*delete\|deprecated"
```

## 📊 Structure Metrics

- **Total files**: $(find . -type f ! -path '*/node_modules/*' ! -path '*/.git/*' | wc -l | tr -d ' ')
- **Directory depth**: $(find . -type d ! -path '*/node_modules/*' ! -path '*/.git/*' | awk -F/ '{print NF-1}' | sort -nr | head -1)
- **Python files**: $(find . -name "*.py" ! -path '*/node_modules/*' | wc -l | tr -d ' ')
- **JavaScript files**: $(find . -name "*.js" -o -name "*.cjs" | ! -path '*/node_modules/*' | wc -l | tr -d ' ')

---
*Generated: $(date)*  
*Script: `./scripts/generate-files-tree.sh`*
EOF

# Move temp file to final location
mv "$TEMP_FILE" "$OUTPUT_FILE"

echo "✅ Generated $OUTPUT_FILE"
echo "📊 $(wc -l < "$OUTPUT_FILE") lines of structure documentation"
echo "🎯 Next: Review comments and identify consolidation opportunities"