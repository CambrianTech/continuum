# Widget Inspect Command

Comprehensive widget inspection command for debugging and autonomous development.

## Overview

The `widget-inspect` command provides detailed analysis of widgets in the browser, including:

- **Content Analysis**: innerHTML, shadowDOM content, attributes
- **Visual State**: dimensions, visibility, positioning, styling
- **DOM Health**: connectivity, parent-child relationships, element counts
- **Performance Metrics**: inspection timing, render state validation

## Usage

### Basic Usage
```bash
./continuum widget-inspect
```

### Custom Selector
```bash
./continuum widget-inspect --selector=".sidebar-widget, .chat-widget"
```

### Lightweight Inspection
```bash
./continuum widget-inspect --includeStyling=false --contentPreviewLength=50
```

## Integration with JTAG

The JTAG CLI uses this command for widget validation:

```typescript
// JTAG CLI automatically delegates to widget-inspect command
const result = await jtag.inspectWidgets('.my-widget');

// Returns detailed widget analysis with UUID tracking
console.log(result.data.inspectionUUID); // inspect-1752170532-abc123
```

## Output Format

```json
{
  "success": true,
  "data": {
    "inspectionUUID": "inspect-1752170532-abc123",
    "result": {
      "inspectionUUID": "inspect-1752170532-abc123", 
      "timestamp": "2025-07-10T18:21:00.000Z",
      "totalWidgets": 2,
      "pageTitle": "continuum",
      "pageUrl": "http://localhost:9000/",
      "selector": "continuum-sidebar, chat-widget",
      "widgets": [
        {
          "index": 0,
          "tagName": "continuum-sidebar",
          "id": null,
          "className": null,
          "isConnected": true,
          "hasContent": false,
          "contentPreview": "[empty]",
          "hasShadowRoot": true,
          "shadowContent": "<!-- Sidebar content here -->",
          "attributes": [],
          "boundingBox": {
            "width": 300,
            "height": 800,
            "x": 0,
            "y": 0,
            "visible": true
          },
          "styling": {
            "display": "block",
            "visibility": "visible", 
            "opacity": "1",
            "backgroundColor": "rgb(255, 255, 255)"
          },
          "childElementCount": 0,
          "parentElement": "div"
        }
      ],
      "performance": {
        "inspectionDuration": 15.2,
        "averageInspectionPerWidget": 7.6
      }
    }
  },
  "message": "Widget inspection completed [UUID: inspect-1752170532-abc123]"
}
```

## Features for Autonomous Development

1. **UUID Tracking**: Every inspection generates a unique identifier for correlation
2. **Error Resilience**: Individual widget failures don't stop overall inspection
3. **Performance Monitoring**: Built-in timing for optimization feedback
4. **Visual Validation**: Confirms widgets are actually visible and rendered
5. **Content Verification**: Validates widgets have expected content

## Testing

Run the integration test:
```bash
cd src/commands/browser/widget-inspect
npm test
```

Used by JTAG integration tests to validate widget health and debugging pipeline.