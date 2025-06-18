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
    local full_path="$3"  # Optional full path for orphan detection
    
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
    
    # Add comment based on file type and connection status
    local comment=""
    # Use full_path if provided, otherwise use file for orphan detection
    local path_for_check="${full_path:-$file}"
    
    # Check for obviously disconnected components (conservative detection)
    local status="connected"  # connected, suspicious, archived, clutter
    case "$path_for_check" in
        # Obviously archived/legacy (actual archive directories)
        archived/*|archive/*) status="archived" ;;
        # TypeScript files (not compiled in this JS-based system)
        *.ts) status="suspicious" ;;
        # Debug/temp files (clearly temporary)
        debug-*.html|temp-*.html|*-temp.*|*.tmp) status="clutter" ;;
        # Coverage reports (generated files that shouldn't be committed)
        coverage/*|htmlcov/*|*.coverage|lcov.info) status="clutter" ;;
        # Node modules and build artifacts (should be gitignored)
        node_modules/*|dist/*|build/*) status="clutter" ;;
        # Log files and OS artifacts
        *.log|.DS_Store|Thumbs.db) status="clutter" ;;
        # IDE and editor files
        .vscode/*|.idea/*|*.swp|*.swo|*~) status="clutter" ;;
        # Python cache and build artifacts
        __pycache__/*|*.pyc|*.pyo|*.egg-info/*) status="clutter" ;;
    esac
    
    # Generate comment based on file type and connection status
    case "$file" in
        *.md) 
            case "$status" in
                "archived") comment="📦 Archived documentation" ;;
                "suspicious") comment="🤔 Documentation (investigate connection)" ;;
                "clutter") comment="🧹 Documentation clutter (should be cleaned?)" ;;
                *) comment="📖 Documentation" ;;
            esac ;;
        *.js|*.cjs) 
            case "$status" in
                "archived") comment="🗄️ Archived JavaScript (filed away)" ;;
                "suspicious") comment="🌀 JavaScript (outside the loop?)" ;;
                "clutter") comment="🧽 JavaScript debris (cleanup needed?)" ;;
                *) comment="⚡ JavaScript/Node.js" ;;
            esac ;;
        *.py) 
            case "$status" in
                "clutter") comment="🧹 Python debris (should be gitignored?)" ;;
                *) comment="🐍 Python" ;;
            esac ;;
        *.json) comment="📋 Configuration/Data" ;;
        *.sh) comment="🔧 Shell Script" ;;
        *.yml|*.yaml) comment="⚙️ YAML Config" ;;
        *.gitignore) comment="🚫 Git ignore rules" ;;
        package*.json) comment="📦 Node.js dependencies" ;;
        requirements*.txt) comment="📦 Python dependencies" ;;
        Dockerfile) comment="🐳 Docker container" ;;
        *test*.py|*test*.js) comment="🧪 Test file" ;;
        *Command.cjs) comment="🎯 Continuum command implementation" ;;
        continuum.cjs) comment="🌟 Main server entry point" ;;
        ai-portal.py) comment="🤖 AI agent interface" ;;
        *.ts)
            comment="🔗 TypeScript (missing from JS loop?)" ;;
        *.html)
            case "$status" in
                "archived") comment="🗃️ Archived HTML (stored away)" ;;
                "suspicious") comment="🌊 HTML (adrift from main flow?)" ;;
                "clutter") comment="🧼 HTML scraps (temp/debug files?)" ;;
                *) comment="🌐 HTML file" ;;
            esac ;;
        *.log)
            comment="🗑️ Log file (should be gitignored!)" ;;
        .DS_Store|Thumbs.db)
            comment="💩 OS junk (delete immediately!)" ;;
        *.tmp|*-temp.*)
            comment="🚮 Temp file (forgot to clean up?)" ;;
        *) 
            if [[ -d "$file" ]]; then
                case "$status" in
                    "archived") comment="📦 Archive directory (isolated)" ;;
                    "suspicious") comment="🏝️ Directory (island - connected?)" ;;
                    "clutter") comment="🗂️ Messy directory (needs cleaning?)" ;;
                    *) comment="📁 Directory" ;;
                esac
            else
                case "$status" in
                    "archived") comment="🗂️ Archived file (shelved)" ;;
                    "suspicious") comment="🎭 File (backstage - not in show?)" ;;
                    "clutter") comment="🧤 File debris (spring cleaning time?)" ;;
                    *) comment="📄 File" ;;
                esac
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

# Extract content after tree section to preserve
PRESERVE_CONTENT=""
if [[ -f "$OUTPUT_FILE" ]]; then
    # Extract everything after the first occurrence of "## 📝 Detailed File Analysis" or similar
    PRESERVE_CONTENT=$(sed -n '/^## 🎯 Agent Study Guide/,$p' "$OUTPUT_FILE" 2>/dev/null || echo "")
    if [[ -z "$PRESERVE_CONTENT" ]]; then
        PRESERVE_CONTENT=$(sed -n '/^## 📸 UI VISUAL DOCUMENTATION/,$p' "$OUTPUT_FILE" 2>/dev/null || echo "")
    fi
    if [[ -z "$PRESERVE_CONTENT" ]]; then
        PRESERVE_CONTENT=$(sed -n '/^## ⚰️ TOMBSTONES/,$p' "$OUTPUT_FILE" 2>/dev/null || echo "")
    fi
fi

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

# Generate tree structure - clean and simple
echo "**File structure overview (detailed analysis in sections below)**" >> "$TEMP_FILE"
echo "" >> "$TEMP_FILE"

# Generate simple clean tree - manual descriptions preserved below
tree -I 'node_modules|.git|__pycache__|\*.pyc|\.DS_Store|\.pytest_cache|htmlcov|\*.egg-info|dist|build|.*|venv|env' \
    --dirsfirst -L 4 | head -100 >> "$TEMP_FILE"

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
        add_comment "$filename" "$indent" "$file" >> "$TEMP_FILE"
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
    add_comment "$filename" "" "$file" >> "$TEMP_FILE"
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

# Skip auto-generating tombstones - they're handled in preserved content
# This prevents duplicate/empty tombstones on top of the rich archaeological ones

# Add footer with commands
# Append preserved content if it exists
if [[ -n "$PRESERVE_CONTENT" ]]; then
    echo "" >> "$TEMP_FILE"
    echo "---" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
    echo "$PRESERVE_CONTENT" >> "$TEMP_FILE"
else
    # Add standard footer only if no preserved content
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
fi

# Move temp file to final location
mv "$TEMP_FILE" "$OUTPUT_FILE"

echo "✅ Generated $OUTPUT_FILE"
echo "📊 $(wc -l < "$OUTPUT_FILE") lines of structure documentation"
echo "🎯 Next: Review comments and identify consolidation opportunities"