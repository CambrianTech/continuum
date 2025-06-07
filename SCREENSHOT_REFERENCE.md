# Continuum Screenshot System Reference

## 📸 AI-Driven Screenshot Capture & Visual Debugging

The Continuum Promise Post Office System enables AI agents to capture, analyze, and act on visual information from web interfaces. This is revolutionary for AI-driven interface control and debugging.

## Quick Start

### Python Client
```bash
# Install dependencies
cd python-client
pip install -e .

# Run basic screenshot example
python examples/simple_screenshot.py

# Try smart element finding
python examples/find_and_capture.py
```

### WebSocket Commands
```bash
# Start Continuum server
node continuum.cjs

# Connect via WebSocket to ws://localhost:9000
# Send screenshot command:
{"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {}"}
```

## Python API Reference

### Basic Usage
```python
from examples.screenshot_capture import ScreenshotCapture

async with ScreenshotCapture() as capture:
    # Capture full page
    result = await capture.capture('body', format='png', open_image=True)
    
    # Smart element finding
    result = await capture.capture('agents', open_image=True)
    
    # Save to file
    result = await capture.capture('#sidebar', save_path='debug/sidebar.png')
```

### Advanced Features
```python
# Multiple formats
await capture.capture('#element', format='jpeg', quality=0.8)
await capture.capture('#element', format='webp', quality=0.9)

# Element search
find_result = await capture.find_element(['sidebar', 'navigation', 'menu'])
if find_result['found']:
    await capture.capture(find_result['selector'])

# Error handling
try:
    result = await capture.capture('#nonexistent')
except Exception as e:
    print(f"Capture failed: {e}")
```

## WebSocket Command Reference

### Screenshot Command
```json
{
  "type": "task", 
  "role": "system", 
  "task": "[CMD:SCREENSHOT] {\"params\": \"options\"}"
}
```

### Parameters
- `selector .class` - Capture specific element by CSS selector
- `100,200,800,600` - Capture specific coordinates (x,y,width,height)  
- `1920x1080 format jpeg quality 0.8` - Set resolution and format
- `format png` - Set image format (png/jpeg/webp)
- `quality 0.9` - Set quality for compressed formats

### Examples
```json
// Full page screenshot
{"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {}"}

// Capture sidebar
{"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {\"params\": \"selector .sidebar\"}"}

// High quality JPEG
{"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {\"params\": \"format jpeg quality 0.9\"}"}

// Specific region
{"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {\"params\": \"100,100,800,600\"}"}
```

## AI Use Cases

### 1. Visual Debugging
```python
async def ai_debug_interface():
    async with ScreenshotCapture() as capture:
        # Capture current state
        current = await capture.capture('body', save_path='debug/current.png')
        
        # AI analyzes image for issues
        issues = await analyze_interface_issues(current['dataURL'])
        
        # AI takes corrective action
        if 'layout_broken' in issues:
            await fix_css_layout()
            
        # Verify fix
        fixed = await capture.capture('body', save_path='debug/fixed.png')
```

### 2. UI Regression Testing
```python
async def ai_regression_test():
    async with ScreenshotCapture() as capture:
        # Capture baseline
        baseline = await capture.capture('#main-content', save_path='test/baseline.png')
        
        # Make changes
        await apply_ui_changes()
        
        # Capture result
        result = await capture.capture('#main-content', save_path='test/result.png')
        
        # AI compares visually
        regression = await ai_visual_diff(baseline['dataURL'], result['dataURL'])
```

### 3. Automated User Experience Monitoring
```python
async def ai_ux_monitor():
    async with ScreenshotCapture() as capture:
        # Continuous monitoring
        while True:
            # Capture key UI areas
            header = await capture.capture('#header')
            sidebar = await capture.capture('#sidebar') 
            content = await capture.capture('#main')
            
            # AI analyzes UX quality
            ux_score = await ai_evaluate_ux([header, sidebar, content])
            
            if ux_score < threshold:
                await ai_improve_ux()
                
            await asyncio.sleep(300)  # Check every 5 minutes
```

### 4. Interface Self-Healing
```python
async def ai_self_healing():
    async with ScreenshotCapture() as capture:
        # Detect visual problems
        screenshot = await capture.capture('body')
        problems = await ai_detect_visual_problems(screenshot['dataURL'])
        
        for problem in problems:
            if problem['type'] == 'missing_element':
                await ai_restore_element(problem['selector'])
            elif problem['type'] == 'layout_shift':
                await ai_fix_layout(problem['area'])
            elif problem['type'] == 'broken_styling':
                await ai_repair_css(problem['element'])
```

## Promise Post Office Architecture

### Flow Diagram
```
AI Agent → WebSocket → Continuum Server → Browser
   ↑                                         ↓
   ← Base64 Image ← Promise Resolution ← html2canvas
```

### Technical Details
1. **AI sends command** via WebSocket with execution ID
2. **Continuum routes** to browser via Promise Post Office System  
3. **Browser loads html2canvas** if needed (automatic CDN loading)
4. **html2canvas captures** element/page as canvas
5. **Canvas converts** to base64 data URL (PNG/JPEG/WebP)
6. **WebSocket returns** image data with matching execution ID
7. **AI receives** complete image for analysis/storage

### Performance
- **Image sizes**: 300x1153 (300KB), 1474x1353 (517KB) typical
- **Transfer time**: <1 second for most screenshots
- **Format efficiency**: PNG (lossless), JPEG (smaller), WebP (best compression)
- **Concurrent support**: Multiple screenshots can be captured simultaneously

## Error Handling

### Common Issues
```python
# Element not found
try:
    await capture.capture('#nonexistent')
except Exception as e:
    # Falls back to body capture
    await capture.capture('body')

# html2canvas loading failure  
# → Automatic retry with different CDN
# → Graceful degradation to text-based analysis

# WebSocket timeout
# → Adjustable timeout values
# → Automatic reconnection logic
```

### Debugging Tips
1. **Use open_image=True** to visually verify captures
2. **Check element visibility** with find_element() first
3. **Try multiple selectors** if specific elements fail
4. **Use body fallback** for reliable full-page capture
5. **Monitor console output** for html2canvas errors

## Integration Examples

### With Continuum Academy
```python
# AI persona that can "see" interfaces
class VisualAI(Persona):
    async def analyze_interface(self):
        async with ScreenshotCapture() as capture:
            screenshot = await capture.capture('body')
            return await self.vision_model.analyze(screenshot['dataURL'])
            
    async def fix_ui_issues(self):
        issues = await self.analyze_interface()
        for issue in issues:
            await self.apply_fix(issue)
```

### With External Vision APIs
```python
import base64
import requests

async def ai_vision_analysis():
    async with ScreenshotCapture() as capture:
        screenshot = await capture.capture('body')
        
        # Extract base64 data
        image_data = screenshot['dataURL'].split(',')[1]
        
        # Send to vision API
        response = requests.post('https://api.openai.com/v1/chat/completions', {
            'model': 'gpt-4-vision-preview',
            'messages': [{
                'role': 'user',
                'content': [
                    {'type': 'text', 'text': 'Analyze this interface for usability issues'},
                    {'type': 'image_url', 'image_url': {'url': screenshot['dataURL']}}
                ]
            }]
        })
```

## Future Capabilities

The screenshot system enables future AI capabilities:

- 🔍 **Visual A/B Testing** - AI-driven interface optimization
- 🎨 **Automated Design** - AI that can see and improve layouts  
- 🔧 **Self-Healing UIs** - Interfaces that fix themselves
- 📊 **Visual Analytics** - Continuous UX monitoring and improvement
- 🤖 **Complete Autonomy** - AI that can fully control web experiences

This is just the beginning of AI-driven web interface control! 🚀