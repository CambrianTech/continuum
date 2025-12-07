# List Command

Data List Command - Shared Types

## Usage

```bash
./jtag list [options]
```

## Parameters

No parameters required.

## Result

Returns execution result.

## Examples

### Basic Usage

```bash
# Example 1: Basic invocation
./jtag list

# Example 2: With parameters
./jtag list --param=value
```

### Programmatic Usage

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('list', {
  // parameters here
});

console.log(result);
```

## Testing

### Unit Tests

```bash
npm run test:unit
```

### Integration Tests

```bash
npm run test:integration
```

## Development

### Build

```bash
npm run build
```

### Lint

```bash
npm run lint
```

## Package

This command is packaged as an npm module and can be installed independently:

```bash
npm install @jtag-commands/list
```

## License

MIT
