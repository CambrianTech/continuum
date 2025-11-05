#!/bin/bash
# Continuum Integration Test Runner with smart venv management

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🧪 Continuum Integration Test Runner${NC}"
echo "=================================="

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTINUUM_DIR="$(dirname "$SCRIPT_DIR")"
VENV_PATH="$CONTINUUM_DIR/.continuum/venv/agents"

echo -e "${YELLOW}📍 Looking for venv: $VENV_PATH${NC}"

# Smart venv setup
setup_venv() {
    if [ ! -d "$VENV_PATH" ]; then
        echo -e "${YELLOW}🔧 Creating virtual environment...${NC}"
        mkdir -p "$(dirname "$VENV_PATH")"
        python3 -m venv "$VENV_PATH"
    fi
    
    echo -e "${YELLOW}⚡ Activating virtual environment...${NC}"
    source "$VENV_PATH/bin/activate"
    
    # Verify activation
    if [ -z "$VIRTUAL_ENV" ]; then
        echo -e "${RED}❌ Failed to activate virtual environment${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✅ Virtual environment active: $VIRTUAL_ENV${NC}"
}

# Smart dependency installation
install_dependencies() {
    # Always install continuum client first
    echo -e "${YELLOW}📦 Installing continuum client...${NC}"
    pip install -e .
    
    # Install from requirements.txt
    if [ -f "$SCRIPT_DIR/requirements.txt" ]; then
        echo -e "${YELLOW}📦 Installing from requirements.txt...${NC}"
        pip install -r requirements.txt
    else
        echo -e "${YELLOW}📦 Installing test dependencies...${NC}"
        pip install pytest pytest-asyncio pytest-cov beautifulsoup4 requests
    fi
    
    # Add coverage and other dev deps
    pip install pytest-cov requests
    
    echo -e "${GREEN}✅ All dependencies installed${NC}"
}

# Setup venv and dependencies
setup_venv
cd "$SCRIPT_DIR"
install_dependencies

# Parse arguments
VERBOSE=""
COVERAGE=""
HTML_REPORT=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--verbose)
            VERBOSE="-v -s"
            shift
            ;;
        -c|--coverage)
            COVERAGE="--cov=continuum_client --cov-report=term-missing"
            shift
            ;;
        --html-report)
            HTML_REPORT="--cov-report=html:htmlcov"
            shift
            ;;
        --unit)
            TEST_TYPE="unit"
            shift
            ;;
        --integration)
            TEST_TYPE="integration" 
            shift
            ;;
        *)
            echo "Unknown option $1"
            exit 1
            ;;
    esac
done

# Default to integration tests
TEST_TYPE=${TEST_TYPE:-"integration"}

# Build pytest command
if [ "$TEST_TYPE" = "unit" ]; then
    echo -e "${BLUE}🔧 Running unit tests...${NC}"
    CMD="python -m pytest tests/unit/ $VERBOSE $COVERAGE $HTML_REPORT --tb=short"
else
    echo -e "${BLUE}🚀 Running integration tests with server management...${NC}"
    CMD="python run_integration_tests.py"
    if [ -n "$VERBOSE" ]; then
        CMD="$CMD --verbose"
    fi
    if [ -n "$COVERAGE" ]; then
        CMD="$CMD --coverage"
    fi
    if [ -n "$HTML_REPORT" ]; then
        CMD="$CMD --html-report"
    fi
fi

echo -e "${YELLOW}🎯 Command: $CMD${NC}"
echo "=================================================================================="

# Run the tests
if eval $CMD; then
    echo "=================================================================================="
    echo -e "${GREEN}🎉 All tests passed!${NC}"
    
    if [ -n "$HTML_REPORT" ]; then
        echo -e "${BLUE}📊 Coverage report: $SCRIPT_DIR/htmlcov/index.html${NC}"
    fi
else
    echo "=================================================================================="
    echo -e "${RED}❌ Tests failed${NC}"
    exit 1
fi