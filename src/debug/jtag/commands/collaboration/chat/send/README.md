# Send Command

Chat Send Command

## Usage

```bash
./jtag send [options]
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message` | `string` | Yes | - |
| `room` | `string` | No | - |
| `senderId` | `UUID` | No | - |
| `replyToId` | `UUID` | No | - |
| `isSystemTest` | `boolean` | No | - |
| `media` | `string[]` | No | - |


## Result

| Field | Type | Description |
|-------|------|-------------|
| `success` | `boolean` | - |
| `message` | `string` | - |
| `messageEntity` | `ChatMessageEntity` | - |
| `shortId` | `string` | - |
| `roomId` | `UUID` | - |


## Examples

### Basic Usage

```bash
# Example 1: Basic invocation
./jtag send

# Example 2: With parameters
./jtag send --param=value
```

### Programmatic Usage

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('send', {
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
npm install @jtag-commands/send
```

## License

MIT
