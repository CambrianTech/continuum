# Validate All Command

Validates all modules in the system for compliance using the middle-out inheritance system.

## Usage
```bash
continuum validate-all
continuum validate-all --fix
continuum validate-all --category=commands
```

## Features
- Uses BaseModule inheritance for validation
- Tests all modules systematically  
- Can auto-fix common compliance issues
- Reports detailed compliance status

## Configuration
```json
{
  "command": "validate-all",
  "category": "Testing",
  "capabilities": ["module-validation", "system-compliance"]
}
```

## Testing
```bash
npm test
```