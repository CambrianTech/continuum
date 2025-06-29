# Validate JS Command

## Definition
- **Name**: validate_js
- **Description**: Validate JavaScript code and syntax
- **Category**: Core
- **Icon**: 🔧
- **Status**: 🟠 UNTESTED (2025-06-18) - Needs documentation and testing
- **Parameters**: `[script] [strict] [browser]`

## Dependencies
```mermaid
graph TD
    A[ValidateJSCommand] --> B[BaseCommand]
    A --> C[ValidateCodeCommand]
    A --> D[JS Parser]
    A --> E[Browser Context]
```

## TODO:
- TODO: Test JavaScript validation
- TODO: Test browser compatibility checks
- TODO: Test syntax error reporting