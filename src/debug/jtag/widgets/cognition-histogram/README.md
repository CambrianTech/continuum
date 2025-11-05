# Cognition Histogram Widget

Real-time AI pipeline visualization showing cognitive processing stages with Winamp-style frequency bars.

![Cognition Histogram](../../../../examples/widget-ui/.continuum/jtag/sessions/user/8dc752cb-5155-4dc0-9b3d-e7d5482f75e7/screenshots/cognition-histogram-final.png)

## Overview

The Cognition Histogram provides instant visual feedback on AI pipeline performance, showing which stages are fast, slow, or bottlenecked during message processing.

## Pipeline Stages

The widget visualizes 5 cognitive stages:

1. **rag-build** - Context gathering from conversation history, memories, and artifacts
2. **should-respond** - Decision-making: should this AI participate in the conversation?
3. **generate** - LLM inference (text generation)
4. **coordination** - ThoughtStream coordination (turn-taking between multiple AIs)
5. **post-response** - Message delivery to database and UI

## Color Legend

- **🟢 fast** - Stage completed faster than baseline (green)
- **🟡 normal** - Stage within expected performance range (yellow)
- **🟠 slow** - Stage taking longer than normal (orange)
- **🔴 stuck** - Bottleneck detected, significant slowdown (red)

## Performance Metrics

Each bar represents:
- **Height**: Percentage of capacity used (0-70% of container)
- **Color**: Performance relative to baseline speed
- **Width**: Fixed at 60% of available space (centered)
- **Glow**: Drop-shadow effect matching bar color

### Baseline Speeds

```typescript
{
  'rag-build': 500ms,
  'should-respond': 200ms,
  'generate': 1000ms,
  'coordination': 300ms,
  'post-response': 100ms
}
```

## Features

- **Real-time updates**: Subscribes to `cognition:stage-complete` events
- **Running averages**: Tracks average duration across multiple events per stage
- **Smooth animation**: `requestAnimationFrame` loop for fluid updates
- **Compact design**: 120px height fits perfectly in sidebar layout
- **Clean visualization**: No text labels on bars, only color-coded legend

## Implementation

### Event Emission

Pipeline stages emit `StageCompleteEvent` with metrics:

```typescript
{
  messageId: UUID,
  personaId: UUID,
  contextId: UUID,
  stage: PipelineStage,
  metrics: {
    durationMs: number,
    resourceUsed: number,
    maxResource: number,
    percentCapacity: number,
    percentSpeed: number,
    status: 'fast' | 'normal' | 'slow' | 'bottleneck'
  },
  timestamp: number
}
```

### Widget Subscription

```typescript
Events.subscribe(COGNITION_EVENTS.STAGE_COMPLETE, (data: StageCompleteEvent) => {
  this.updateStageData(data);
});
```

## Usage

Widget is automatically included in the sidebar:

```html
<sidebar-widget>
  <continuum-emoter-widget></continuum-emoter-widget>
  <cognition-histogram-widget></cognition-histogram-widget>
  <continuum-metrics-widget></continuum-metrics-widget>
</sidebar-widget>
```

## Development

Hot CSS reloading for rapid iteration:

```bash
./jtag debug/widget-css --widgetSelector="cognition-histogram-widget" --cssContent="..."
```

## Architecture

- **TypeScript**: `CognitionHistogramWidget.ts` - Event handling, data aggregation, SVG rendering
- **HTML**: `cognition-histogram.html` - Widget template with legend
- **CSS**: `cognition-histogram.css` - Styling, legend, animations
- **Events**: `CognitionEventTypes.ts` - Event schemas and baseline definitions

## Future Enhancements

Potential additional visualization modes:
- AI adapters/models status
- Cadence convergence over time (line graph)
- Bottleneck analysis (sorted by duration)
- Token usage tracking
- Response time trends
