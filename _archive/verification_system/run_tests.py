#!/usr/bin/env python3
"""
Test runner for Continuum Verification System

Runs all unit tests for the verification package modules.
"""

import unittest
import sys
from pathlib import Path

# Add the verification system to path for testing
sys.path.insert(0, str(Path(__file__).parent))

def run_tests():
    """Discover and run all tests in the tests directory"""
    # Discover tests
    loader = unittest.TestLoader()
    test_dir = Path(__file__).parent / 'tests'
    suite = loader.discover(str(test_dir), pattern='test_*.py')
    
    # Run tests
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    
    # Return exit code based on results
    return 0 if result.wasSuccessful() else 1

if __name__ == '__main__':
    sys.exit(run_tests())