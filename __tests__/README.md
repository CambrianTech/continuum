# Continuum Test Structure

This document describes the reorganized test structure for the Continuum project, which integrates both JavaScript and Python tests under a unified framework.

## Directory Structure

```
__tests__/
├── unit/                           # Unit tests (single component/function)
│   ├── js/                        # JavaScript unit tests
│   │   ├── commands/              # Command system unit tests
│   │   ├── components/            # UI component unit tests
│   │   ├── core/                  # Core functionality unit tests
│   │   └── utils/                 # Utility function unit tests
│   └── python/                    # Python unit tests
│       ├── client/                # Client functionality tests
│       ├── utils/                 # Python utility tests
│       └── core/                  # Core Python module tests
├── integration/                   # Cross-system integration tests
│   ├── ai/                        # AI capabilities integration
│   ├── api/                       # API integration tests
│   ├── commands/                  # Command system integration
│   ├── screenshot/                # Screenshot functionality (JS + Python)
│   ├── ui/                        # UI integration tests
│   └── websocket/                 # WebSocket communication
├── functional/                    # Feature-complete functional tests
│   ├── user-scenarios/            # User scenario tests
│   ├── visual/                    # Visual testing
│   └── workflows/                 # End-to-end workflows
├── comprehensive/                 # Full system tests
│   └── system-integration/        # Complete system tests
├── critical/                      # Critical path tests
│   └── core-functionality/       # Must-pass tests
├── fixtures/                      # Shared test fixtures
│   ├── configs/                   # Test configurations
│   ├── data/                      # Test data
│   └── mocks/                     # Mock objects
└── config/                        # Test configuration files
    ├── jest.config.cjs            # Jest configuration
    ├── jest.global-setup.js       # Jest global setup
    ├── jest.global-teardown.js    # Jest global teardown
    ├── pytest.ini                # PyTest configuration
    └── test-runner.cjs            # Unified test runner
```

## Test Organization Strategy

### Functional Grouping over Language Separation

Tests are organized by **functionality** rather than by programming language:

- **Screenshot tests**: Both JS and Python tests for screenshot functionality are in `integration/screenshot/`
- **Command tests**: Cross-language command testing in `integration/commands/`
- **WebSocket tests**: Communication tests in `integration/websocket/`

### Test Categories

1. **Unit Tests**: Single component/function testing
   - Fast execution
   - No external dependencies
   - Language-specific directories

2. **Integration Tests**: Cross-component testing
   - Multiple components working together
   - Mixed JS/Python tests for same functionality
   - Real system interactions

3. **Functional Tests**: Feature-complete testing
   - End-to-end workflows
   - User scenarios
   - Visual validation

4. **Comprehensive Tests**: Full system testing
   - Complete system integration
   - Performance testing
   - Load testing

5. **Critical Tests**: Must-pass tests
   - Core functionality verification
   - Blocking tests for CI/CD

## Running Tests

### Unified Test Commands

```bash
# Run all tests (JS + Python)
npm test

# Run specific test categories
npm run test:unit          # All unit tests
npm run test:integration   # All integration tests
npm run test:functional    # All functional tests
npm run test:comprehensive # All comprehensive tests
npm run test:critical      # Critical tests only

# Run by functionality
npm run test:screenshot    # Screenshot functionality tests
npm run test:commands      # Command system tests
npm run test:websocket     # WebSocket communication tests
npm run test:ui            # UI tests (JS + Python)
npm run test:ai            # AI capabilities tests

# Run by language
npm run test:unit:js       # JavaScript unit tests only
npm run test:unit:python   # Python unit tests only
npm run test:integration:js     # JavaScript integration tests
npm run test:integration:python # Python integration tests

# Watch mode and coverage
npm run test:watch         # Watch mode for development
npm run test:coverage      # Generate coverage reports
npm run test:coverage:js   # JavaScript coverage only
npm run test:coverage:python # Python coverage only
```

### Advanced Test Runner

The unified test runner (`__tests__/config/test-runner.cjs`) provides:

- Cross-language test execution
- Unified reporting
- Functional test grouping
- Dependency management between JS and Python tests

```bash
# Use the test runner directly
node __tests__/config/test-runner.cjs unit
node __tests__/config/test-runner.cjs screenshot
node __tests__/config/test-runner.cjs commands
```

## Configuration Files

### Jest Configuration (`__tests__/config/jest.config.cjs`)
- Configured for ES modules
- Ignores problematic directories
- Unified coverage reporting
- Global setup/teardown

### PyTest Configuration (`__tests__/config/pytest.ini`)
- Markers for functional grouping
- Coverage configuration
- Async test support
- Unified directory structure

## Test Markers (Python)

Python tests use markers for categorization:

```python
@pytest.mark.unit          # Unit tests
@pytest.mark.integration   # Integration tests
@pytest.mark.functional    # Functional tests
@pytest.mark.screenshot    # Screenshot functionality
@pytest.mark.commands      # Command system
@pytest.mark.websocket     # WebSocket communication
@pytest.mark.slow          # Slow tests (>5 seconds)
```

## Best Practices

### 1. Functional Co-location
Place tests for the same functionality together, regardless of language:
```
integration/screenshot/
├── screenshot-pipeline.test.py    # Python screenshot tests
├── screenshot-capture.test.js     # JavaScript screenshot tests
└── visual-validation.test.py      # Cross-language validation
```

### 2. Shared Fixtures
Use the `fixtures/` directory for shared test data:
```
fixtures/
├── data/sample-images/
├── mocks/api-responses/
└── configs/test-environments/
```

### 3. Cross-language Testing
For features that span both languages, create integration tests that verify end-to-end functionality.

### 4. Test Naming
- **JavaScript**: Use `.test.js` or `.test.cjs` extensions
- **Python**: Use `test_*.py` naming convention
- **Descriptive names**: `screenshot-pipeline.test.py` not `test1.py`

## Migration Status

✅ **Completed**:
- New directory structure created
- Python tests moved and organized
- JavaScript tests reorganized by function
- Unified test runner implemented
- npm scripts updated
- Configuration files created

⚠️ **Known Issues**:
- Some import paths need updating after reorganization
- A few tests may need path adjustments
- Coverage reporting may need tuning

## Future Enhancements

1. **Cross-language Test Dependencies**: Ability to run Python setup for JS tests
2. **Parallel Execution**: Run JS and Python tests in parallel
3. **Advanced Reporting**: Unified HTML reports combining both languages
4. **Test Data Management**: Shared test database/fixtures
5. **CI/CD Integration**: Optimized pipeline using the new structure

## Contributing

When adding new tests:

1. **Choose the right category**: Unit, Integration, Functional, etc.
2. **Group by functionality**: Place related tests together
3. **Use appropriate markers**: Tag Python tests with relevant markers
4. **Follow naming conventions**: Clear, descriptive names
5. **Update documentation**: Add new test categories to this README

## Support

For questions about the test structure:
- Check this README first
- Review existing tests for examples
- Run `npm run test:help` for command reference
- Check the test runner source: `__tests__/config/test-runner.cjs`