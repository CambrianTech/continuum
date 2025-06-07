#!/usr/bin/env python3
"""
Continuum Integration Test Runner
Manages server lifecycle and runs all integration tests

🚨 VIRTUAL ENVIRONMENT REQUIRED 🚨

Usage:
    python run_integration_tests.py [--verbose] [--coverage] [--html-report]
    
Server Management:
    - Automatically starts/restarts Continuum server
    - Waits for server to be ready before running tests
    - Cleans up server after tests complete
"""

import sys
import subprocess
import argparse
from continuum_client import ContinuumServerManager

def check_virtual_env():
    """Check if running in virtual environment"""
    in_venv = (
        hasattr(sys, 'real_prefix') or 
        (hasattr(sys, 'base_prefix') and sys.base_prefix != sys.prefix)
    )
    
    if not in_venv:
        print("🚨 ERROR: Not running in virtual environment!")
        print()
        print("Virtual environment is MANDATORY for these tests.")
        print()
        print("Setup steps:")
        print("1. python -m venv continuum_test_env")
        print("2. source continuum_test_env/bin/activate  # Linux/Mac")
        print("   OR continuum_test_env\\Scripts\\activate  # Windows")
        print("3. pip install -e .")
        print("4. pip install -e .[dev]")
        print("5. python run_integration_tests.py")
        print()
        sys.exit(1)
    
    print("✅ Virtual environment detected")

def check_dependencies():
    """Check if required dependencies are installed"""
    required_packages = {
        'pytest': 'pytest',
        'pytest-asyncio': 'pytest_asyncio', 
        'websockets': 'websockets',
        'beautifulsoup4': 'bs4'
    }
    missing_packages = []
    
    for package_name, import_name in required_packages.items():
        try:
            __import__(import_name)
        except ImportError:
            missing_packages.append(package_name)
    
    if missing_packages:
        print(f"🚨 Missing required packages: {', '.join(missing_packages)}")
        print("Install with: pip install -e .[dev]")
        sys.exit(1)
    
    print("✅ All dependencies available")

def run_tests_with_server(verbose=False, coverage=False, html_report=False):
    """Run integration tests with managed server"""
    
    print("🔧 Managing Continuum server for tests...")
    
    with ContinuumServerManager() as server:
        print("✅ Continuum server is ready")
        
        # Build pytest command - run all tests (integration and unit)
        cmd = ['python', '-m', 'pytest', 'tests/']
        
        if verbose:
            cmd.extend(['-v', '-s'])
        
        if coverage:
            cmd.extend(['--cov=continuum_client', '--cov-report=term-missing'])
            if html_report:
                cmd.append('--cov-report=html:htmlcov')
        
        cmd.extend(['-m', 'not slow', '--tb=short'])
        
        print(f"🧪 Running tests: {' '.join(cmd)}")
        print("=" * 80)
        
        try:
            result = subprocess.run(cmd, check=True)
            print("=" * 80)
            print("✅ All integration tests passed!")
            return True
            
        except subprocess.CalledProcessError as e:
            print("=" * 80)
            print(f"❌ Tests failed with exit code {e.returncode}")
            return False

def main():
    """Main test runner entry point"""
    parser = argparse.ArgumentParser(
        description="Run Continuum integration tests with server management",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )
    
    parser.add_argument('--verbose', '-v', action='store_true', help='Verbose test output')
    parser.add_argument('--coverage', '-c', action='store_true', help='Generate code coverage report')
    parser.add_argument('--html-report', action='store_true', help='Generate HTML coverage report')
    parser.add_argument('--list-tests', action='store_true', help='List available tests')
    
    args = parser.parse_args()
    
    print("🧪 Continuum Integration Test Runner")
    print("=" * 40)
    
    # Essential checks
    check_virtual_env()
    check_dependencies()
    
    if args.list_tests:
        cmd = ['python', '-m', 'pytest', '--collect-only', '-q', 'tests/integration/']
        subprocess.run(cmd)
        return
    
    # Run tests with server management
    success = run_tests_with_server(
        verbose=args.verbose,
        coverage=args.coverage,
        html_report=args.html_report
    )
    
    if not success:
        sys.exit(1)
    
    print()
    print("🎉 Integration testing complete!")
    print()
    print("Test Coverage:")
    print("✅ Agent registration and UI updates")
    print("✅ Promise-based JavaScript execution")
    print("✅ Error handling and promise rejection")
    print("✅ Server restart and crash recovery")
    print("✅ HTML parsing and DOM verification")
    print("✅ Concurrent operations and routing")

if __name__ == '__main__':
    main()