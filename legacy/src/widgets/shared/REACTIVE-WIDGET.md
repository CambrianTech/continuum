# ReactiveWidget - Efficient React-like Components

## Overview

ReactiveWidget brings React-like efficiency to web components using [Lit](https://lit.dev). Instead of destroying and recreating DOM with `innerHTML`, it:

1. **Diffs templates** - Only updates what changed
2. **Preserves focus/scroll** - Input state survives re-renders
3. **Batches updates** - Multiple state changes = one render
4. **Automatic cleanup** - Event listeners removed on disconnect

## Quick Start

```typescript
import { ReactiveWidget, html, css } from './ReactiveWidget';

class MyCounter extends ReactiveWidget {
  // Declare reactive state
  static override properties = {
    ...ReactiveWidget.properties,
    count: { type: Number, state: true }
  };

  count = 0;

  // Styles are scoped to this component
  static override styles = css`
    button { padding: 8px 16px; }
    .count { font-size: 24px; color: var(--color-primary); }
  `;

  // Render is called whenever state changes
  protected renderContent() {
    return html`
      <div class="count">${this.count}</div>
      <button @click=${() => this.count++}>+1</button>
    `;
  }
}

customElements.define('my-counter', MyCounter);
```

## Key Differences from BaseWidget

| BaseWidget (old) | ReactiveWidget (new) |
|------------------|----------------------|
| `this.shadowRoot.innerHTML = \`...\`` | `return html\`...\`` |
| Manual DOM queries | Template bindings |
| Manual event listeners | `@click=${handler}` |
| No diff - full replace | Smart diffing |
| Focus lost on update | Focus preserved |

## Declaring Reactive Properties

Use static `properties` to declare state that triggers re-renders:

```typescript
class MyWidget extends ReactiveWidget {
  static override properties = {
    ...ReactiveWidget.properties,  // Include base properties
    // Internal state (not reflected to attributes)
    items: { type: Array, state: true },
    loading: { type: Boolean, state: true },

    // Public properties (can be set via attributes)
    title: { type: String },
    maxItems: { type: Number, attribute: 'max-items' }
  };

  // Initialize with defaults
  items: string[] = [];
  loading = false;
  title = 'My Widget';
  maxItems = 10;
}
```

### Property Options

| Option | Description |
|--------|-------------|
| `type` | `String`, `Number`, `Boolean`, `Array`, `Object` |
| `state` | If true, internal only (not an attribute) |
| `attribute` | Custom attribute name (default: lowercased prop name) |
| `reflect` | Reflect property back to attribute |

## Template Syntax

Lit uses tagged template literals with special bindings:

```typescript
html`
  <!-- Text content -->
  <h1>${this.title}</h1>

  <!-- Attribute binding -->
  <input type="text" placeholder="${this.placeholder}">

  <!-- Property binding (note the dot) -->
  <input .value=${this.inputValue}>

  <!-- Boolean attribute -->
  <button ?disabled=${this.loading}>Submit</button>

  <!-- Event handler -->
  <button @click=${this.handleClick}>Click Me</button>
  <input @input=${(e) => this.value = e.target.value}>

  <!-- Class map -->
  <div class="card ${this.active ? 'active' : ''}">

  <!-- Conditional rendering -->
  ${this.loading ? html`<div class="spinner"></div>` : html`<div>${this.content}</div>`}

  <!-- List rendering -->
  ${this.items.map(item => html`<li>${item.name}</li>`)}
`
```

## Lifecycle Hooks

```typescript
class MyWidget extends ReactiveWidget {
  // Called when added to DOM
  protected onConnect(): void {
    console.log('Widget connected');
  }

  // Called when removed from DOM
  protected onDisconnect(): void {
    console.log('Widget disconnected');
  }

  // Called after first render
  protected onFirstRender(): void {
    console.log('First render complete');
  }

  // Called after every render (use sparingly)
  protected updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);
    if (changedProperties.has('items')) {
      console.log('Items changed!');
    }
  }
}
```

## Async Operations

Use `withLoading` for async operations:

```typescript
class DataWidget extends ReactiveWidget {
  async loadData() {
    await this.withLoading(async () => {
      const result = await this.executeCommand('data/list', {
        collection: 'users'
      });
      this.users = result.items;
    });
  }

  protected renderContent() {
    // this.loading and this.error are handled automatically
    return html`
      <ul>
        ${this.users.map(u => html`<li>${u.name}</li>`)}
      </ul>
    `;
  }
}
```

## Command Execution

```typescript
// Simple command
const users = await this.executeCommand<DataListParams, DataListResult>('data/list', {
  collection: 'users'
});

// Cached command (30s default TTL)
const config = await this.cachedCommand('config/get', { key: 'theme' });

// Clear cache
this.clearCache('data/list'); // Clear specific
this.clearCache();            // Clear all
```

## Positron Context

Emit context for AI awareness:

```typescript
this.emitContext(
  {
    widgetType: 'settings',
    section: 'ai-providers',
    title: 'AI Provider Settings'
  },
  {
    action: 'configuring',
    target: 'API keys'
  }
);
```

## Migration Guide

### Step 1: Change Base Class

```typescript
// Before
class MyWidget extends BaseWidget {

// After
class MyWidget extends ReactiveWidget {
```

### Step 2: Add Static Properties

```typescript
// Add at top of class
static override properties = {
  ...ReactiveWidget.properties,
  myState: { type: String, state: true }
};
```

### Step 3: Replace innerHTML with render

```typescript
// Before
async renderWidget() {
  this.shadowRoot.innerHTML = `
    <div>${this.data}</div>
  `;
}

// After
protected renderContent() {
  return html`
    <div>${this.data}</div>
  `;
}
```

### Step 4: Replace Manual Events

```typescript
// Before
this.shadowRoot.querySelector('button').addEventListener('click', () => {
  this.handleClick();
});

// After (in template)
html`<button @click=${() => this.handleClick()}>Click</button>`
```

### Step 5: Replace Query Selectors

```typescript
// Before
const input = this.shadowRoot.querySelector('input');
this.value = input.value;

// After (use bindings)
html`<input .value=${this.value} @input=${e => this.value = e.target.value}>`
```

## Best Practices

1. **Immutable updates** - Create new objects/arrays for state changes:
   ```typescript
   // Wrong - won't trigger re-render
   this.items.push(item);

   // Right - creates new array
   this.items = [...this.items, item];
   ```

2. **Avoid query selectors** - Use template bindings instead

3. **Small, focused components** - Easier to optimize

4. **Use `state: true`** - For internal state that shouldn't be attributes

5. **Batch updates** - Lit automatically batches, but avoid updating in loops

## Performance Tips

- Use `repeat()` directive for keyed lists
- Use `cache()` directive for conditional content
- Use `guard()` directive to prevent unnecessary re-renders
- Profile with Chrome DevTools Performance tab

## Full Example: Form Widget

See `examples/ReactiveFormExample.ts` for a complete form implementation with:
- Reactive form state
- Validation
- Error handling
- Positron context emission
- Debug visualization
