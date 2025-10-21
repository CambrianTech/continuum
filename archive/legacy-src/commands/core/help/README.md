# Help Command

Provides help and documentation for Continuum system commands.

## Usage
```bash
continuum help [command]
continuum help --all
```

## Examples
```bash
continuum help                    # Show general help
continuum help file-read         # Show help for specific command
continuum help --category=file   # Show help for file commands
```

## Configuration
```json
{
  "command": "help",
  "category": "Core",
  "capabilities": ["help-generation", "documentation"]
}
```

## Testing
```bash
npm test
```