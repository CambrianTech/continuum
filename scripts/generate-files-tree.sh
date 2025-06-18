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
    
    # Check if we have existing comments for this file - look for various patterns
    local existing_comment=""
    if [[ -f "$OUTPUT_FILE" ]]; then
        # Try to find existing comment from detailed section
        existing_comment=$(grep -A3 "^### ${file}.*{#.*}" "$OUTPUT_FILE" 2>/dev/null | grep "^  # " | sed "s/^  # //" | head -1 || echo "")
        
        # If no detailed comment, try tree section
        if [[ -z "$existing_comment" ]]; then
            existing_comment=$(grep "\\[${file}\\].*# " "$OUTPUT_FILE" 2>/dev/null | sed 's/.*# //' || echo "")
        fi
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
    
    # Use existing comment if available, otherwise use default
    if [[ -n "$existing_comment" ]]; then
        comment="$existing_comment"
    fi
    
    echo "$indent  # $comment"
}

# Function to add tombstone for deleted files
add_tombstone() {
    local deleted_file="$1"
    local indent="$2"
    
    # Get git info about when file was deleted
    local git_info=""
    if git log --oneline --follow -- "$deleted_file" 2>/dev/null | head -1 | grep -q .; then
        local last_commit=$(git log --oneline --follow -- "$deleted_file" 2>/dev/null | head -1 | cut -d' ' -f1)
        local commit_date=$(git show -s --format=%ci "$last_commit" 2>/dev/null | cut -d' ' -f1)
        git_info=" (last seen: $commit_date, commit: $last_commit)"
    fi
    
    echo "${indent}### 🪦 ${deleted_file} {#tombstone-$(echo "$deleted_file" | sed 's|/|-|g' | tr '[:upper:]' '[:lower:]')}"
    echo "${indent}  # 🪦 DELETED FILE - Removed $(date +%Y-%m-%d)${git_info}"
    echo ""
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

# Generate tree structure with filtering and markdown links
echo "🔗 **Click any file below to jump to detailed comments**" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Generate tree with links and one-liner comments
tree -a -I 'node_modules|.git|__pycache__|\*.pyc|\.DS_Store|\.pytest_cache|htmlcov|\*.egg-info|dist|build' \
    --dirsfirst | while IFS= read -r line; do
    # Extract just the filename from tree structure
    filename=$(echo "$line" | sed -E 's/^[│├└─ ]+//')
    
    if [[ -f "$filename" && "$filename" != "." ]]; then
        # Convert filename to markdown anchor
        anchor=$(echo "$filename" | sed 's|/|-|g' | sed 's/[^a-zA-Z0-9._-]/-/g' | tr '[:upper:]' '[:lower:]')
        # Get existing comment if available
        existing_comment=""
        if [[ -f "$OUTPUT_FILE" ]]; then
            existing_comment=$(grep -A1 "^### ${filename}" "$OUTPUT_FILE" 2>/dev/null | tail -n1 | grep "^  # " | sed "s/^  # //" || echo "")
        fi
        
        # Add comment based on file type
        comment=""
        case "$filename" in
            *.md) comment="📖 Documentation" ;;
            *.js|*.cjs) comment="⚡ JavaScript/Node.js" ;;
            *.py) comment="🐍 Python" ;;
            *.json) comment="📋 Configuration/Data" ;;
            *.sh) comment="🔧 Shell Script" ;;
            *) comment="📄 File" ;;
        esac
        
        # Use existing comment if available
        if [[ -n "$existing_comment" ]]; then
            comment="$existing_comment"
        fi
        
        # Replace filename in line with markdown link and add comment
        prefix=$(echo "$line" | sed -E 's/[^│├└─ ].*//')
        echo "${prefix}[${filename}](#${anchor}) # ${comment}"
    else
        # Keep directory structure as-is
        echo "$line"
    fi
done >> "$TEMP_FILE"

echo "" >> "$TEMP_FILE"

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
        # Create markdown anchor for linking
        anchor=$(echo "$file" | sed 's|/|-|g' | sed 's/[^a-zA-Z0-9._-]/-/g' | tr '[:upper:]' '[:lower:]')
        echo "${indent}### ${filename} {#${anchor}}" >> "$TEMP_FILE"
        add_comment "$filename" "$indent" >> "$TEMP_FILE"
        echo "" >> "$TEMP_FILE"
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
    # Create markdown anchor for linking
    anchor=$(echo "$filename" | sed 's|/|-|g' | sed 's/[^a-zA-Z0-9._-]/-/g' | tr '[:upper:]' '[:lower:]')
    echo "### ${filename} {#${anchor}}" >> "$TEMP_FILE"
    add_comment "$filename" "" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
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

# Check for deleted files and add tombstones
if [[ -f "$OUTPUT_FILE" ]]; then
    echo "" >> "$TEMP_FILE"
    echo "## 🪦 Deleted Files (Tombstones)" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
    
    # Extract file paths from previous FILES.md detailed sections
    grep "^### .*{#.*}$" "$OUTPUT_FILE" | sed 's/^### \(.*\) {#.*}$/\1/' | while read -r old_file; do
        # Skip tombstone entries and check if file still exists
        if [[ ! "$old_file" =~ ^🪦 ]] && [[ ! -f "$old_file" ]] && [[ ! -d "$old_file" ]]; then
            add_tombstone "$old_file" "" >> "$TEMP_FILE"
        fi
    done
fi

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