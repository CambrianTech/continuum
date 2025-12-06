# Generate Command

Generate a new command from a CommandSpec JSON definition

## Usage

```bash
./jtag generate --spec=<value>
```

## Parameters

- **spec** (required): `object` - CommandSpec object defining the command to generate
- **template** (optional): `boolean` - Return an example CommandSpec template instead of generating

## Result

Returns CommandResult with:
- **filesCreated**: `array` - Array of file paths that were created
- **commandPath**: `string` - Base directory path where command was generated
- **templateSpec**: `object` - Example CommandSpec template (only when template=true)

## Examples

### Get example template

```bash
./jtag generate --template=true
```

**Expected result:**
{ templateSpec: {...}, filesCreated: [], commandPath: "" }

### Generate command from spec

```bash
./jtag generate --spec='{"name":"mycommand","description":"My command",...}'
```

**Expected result:**
{ filesCreated: [...], commandPath: "commands/mycommand" }

## Access Level

**internal** - Internal use only, not exposed to AI personas

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/GenerateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenerateBrowser.ts`
- **Server**: Server-specific implementation in `server/GenerateServer.ts`
