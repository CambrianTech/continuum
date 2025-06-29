# ASCII Diagram Command

Generate properly aligned ASCII art diagrams for documentation and visual debugging.

## Purpose

Creates ASCII art boxes and diagrams that maintain perfect alignment across different terminal widths and markdown renderers. Essential for documentation that needs to look professional in both CLI and web contexts.

## Usage

```bash
# Generate simple text box
python3 python-client/ai-portal.py --cmd ascii_diagram --params '{"type": "simple", "content": ["Hello World"]}'

# Generate command bus architecture
python3 python-client/ai-portal.py --cmd ascii_diagram --params '{"type": "command-bus", "output_format": "markdown"}'

# Generate flow diagram
python3 python-client/ai-portal.py --cmd ascii_diagram --params '{"type": "flow", "title": "Process Flow", "content": ["Step 1", "Step 2", "Step 3"]}'
```

## Parameters

- `type`: Diagram type (`simple`, `flow`, `table`, `command-bus`)
- `content`: Array of content lines
- `output_format`: Output format (`plain`, `markdown`)
- `headers`: Table headers (for table type)
- `rows`: Table rows (for table type)
- `title`: Title for flow diagrams

## Examples

### Simple Box
```
┌─────────────┐
│ Hello World │
└─────────────┘
```

### Command Bus Architecture
```
┌─────────────────────────────────────────┐
│           Continuum Server              │
│         (Orchestrator)                  │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌────────┐ ┌─────────┐ ┌────┐ ┌────┐ │ │
│  │  │Academy │ │Screenshot│ │Chat│ │Help│ │ │
│  │  │        │ │         │ │    │ │    │ │ │
│  │  └────────┘ └─────────┘ └────┘ └────┘ │ │
│  └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
         ↑                    ↑
   ┌─────────┐          ┌─────────┐
   │   AI    │          │ Browser │
   │ Portal  │          │   UI    │
   │(Python) │          │(WebApp) │
   └─────────┘          └─────────┘
```

## Technical Notes

- Uses proper Unicode box-drawing characters
- Validates alignment automatically  
- Supports markdown code block export
- Integrates with Continuum command system
- Python-based core with CJS command wrapper

## JTAG Debugging

Perfect for visual verification in git hooks and automated documentation. ASCII output can be captured in logs and compared for regression testing.