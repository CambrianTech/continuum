# Ping Command

Include detailed AI persona health status

## Usage

```bash
./jtag ping [options]
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `server` | `ServerEnvironmentInfo` | No | - |
| `browser` | `BrowserEnvironmentInfo` | No | - |
| `verbose` | `boolean` | No | - |


## Result

| Field | Type | Description |
|-------|------|-------------|
| `success` | `boolean` | - |
| `server` | `ServerEnvironmentInfo` | - |
| `browser` | `BrowserEnvironmentInfo` | - |
| `timestamp` | `string` | - |
| `aiStatus` | `{` | - |
| `total` | `number` | - |
| `healthy` | `number` | - |
| `starting` | `number` | - |
| `degraded` | `number` | - |
| `dead` | `number` | - |
| `checkDuration` | `number` | - |


## Examples

### Basic Usage

```bash
# Example 1: Basic invocation
./jtag ping

# Example 2: With parameters
./jtag ping --param=value
```

### Programmatic Usage

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ping', {
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
npm install @jtag-commands/ping
```

## License

MIT
