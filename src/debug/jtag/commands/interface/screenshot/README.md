# Screenshot Command

Screenshot Command - Shared Types

## Usage

```bash
./jtag screenshot [options]
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `filename` | `string` | No | - |
| `selector` | `string` | No | - |
| `querySelector` | `string` | No | - |
| `elementName` | `string` | No | - |
| `options` | `ScreenshotOptions` | No | - |
| `cropX` | `number` | No | - |
| `cropY` | `number` | No | - |
| `cropWidth` | `number` | No | - |
| `cropHeight` | `number` | No | - |
| `width` | `number` | No | - |
| `height` | `number` | No | - |
| `scale` | `number` | No | - |
| `quality` | `number` | No | - |
| `maxFileSize` | `number` | No | - |
| `format` | `ScreenshotFormat` | No | - |
| `destination` | `ScreenshotDestination` | No | - |
| `resultType` | `ResultType` | Yes | - |
| `dataUrl` | `string` | No | - |
| `metadata` | `ScreenshotMetadata` | No | - |


## Result

| Field | Type | Description |
|-------|------|-------------|
| `success` | `boolean` | - |
| `filepath` | `string` | - |
| `filename` | `string` | - |
| `timestamp` | `string` | - |
| `options` | `ScreenshotOptions` | - |
| `error` | `JTAGError` | - |
| `metadata` | `ScreenshotMetadata` | - |
| `dataUrl` | `string` | - |
| `bytes` | `Uint8Array` | - |
| `media` | `MediaItem` | - |


## Examples

### Basic Usage

```bash
# Example 1: Basic invocation
./jtag screenshot

# Example 2: With parameters
./jtag screenshot --param=value
```

### Programmatic Usage

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('screenshot', {
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
npm install @jtag-commands/screenshot
```

## License

MIT
