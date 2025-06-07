# Continuum Python Client Examples

This directory contains practical examples of using the Continuum Python Client for real-world tasks.

## Screenshot Capture Examples

### 📸 [`simple_screenshot.py`](simple_screenshot.py)
Quick example showing basic screenshot capture and auto-open functionality.

```bash
python simple_screenshot.py
```

**What it does:**
- Captures full page screenshot
- Opens image automatically in default viewer
- Shows image dimensions and data size

### 🔍 [`find_and_capture.py`](find_and_capture.py) 
Advanced example showing element searching and targeted capture.

```bash
python find_and_capture.py
```

**What it does:**
- Searches for elements using multiple strategies
- Tries text content search, ID matching, class matching
- Captures specific UI components
- Saves to files with descriptive names

### 🎛️ [`screenshot_capture.py`](screenshot_capture.py)
Full-featured screenshot capture class with all options.

**Features:**
- Smart element finding by search terms
- Multiple image formats (PNG, JPEG, WebP)
- Quality control for compressed formats
- Auto-save and auto-open options
- Error handling and fallbacks

## Quick Usage Patterns

### Basic Capture
```python
from screenshot_capture import ScreenshotCapture

async with ScreenshotCapture() as capture:
    result = await capture.capture('body', format='png', open_image=True)
```

### Find and Capture
```python
# Search for element containing text
result = await capture.capture('agents', open_image=True)

# Or find first, then capture
find_result = await capture.find_element('sidebar')
if find_result['found']:
    result = await capture.capture(find_result['selector'])
```

### Save to File
```python
result = await capture.capture(
    selector='#main-content',
    format='jpeg',
    quality=0.8,
    save_path='screenshots/content.jpg'
)
```

### Multiple Formats
```python
formats = ['png', 'jpeg', 'webp']
for fmt in formats:
    await capture.capture('#element', format=fmt, save_path=f'image.{fmt}')
```

## How It Works

The screenshot system uses the **Promise Post Office System**:

1. **Python** sends JavaScript to browser via WebSocket
2. **Browser** loads html2canvas library if needed  
3. **Browser** captures element using html2canvas
4. **Browser** converts to base64 data URL
5. **Browser** sends result back via WebSocket with execution ID
6. **Python** receives base64 image data with proper routing
7. **Python** can save to file or display immediately

This provides true promise-like screenshot capture with:
- ✅ Proper error handling
- ✅ Timeout support  
- ✅ Concurrent capture support
- ✅ Real image data transfer
- ✅ Cross-platform file handling

## Advanced Features

### Element Finding Strategies
The system tries multiple approaches to find elements:

1. **Direct CSS selector** - `#id`, `.class`, `tag`
2. **ID contains** - `[id*="search"]` 
3. **Class contains** - `[class*="search"]`
4. **Text content search** - Elements containing text
5. **Fallback to body** - If nothing found

### Image Formats
- **PNG** - Lossless, larger files, best quality
- **JPEG** - Lossy compression, smaller files, quality control
- **WebP** - Modern format, best compression, quality control

### Error Handling
- Missing elements → automatic fallback
- html2canvas load errors → graceful failure  
- WebSocket timeouts → proper error messages
- File save errors → detailed error info

## Requirements

- Continuum server running on configured port
- Browser with WebSocket support
- Internet connection (for html2canvas CDN)
- Python 3.7+ with asyncio

## Tips

1. **Use descriptive search terms** - 'agents', 'sidebar', 'content' work better than 'div'
2. **Try multiple approaches** - If CSS selector fails, try text search
3. **Check element visibility** - Hidden elements may capture as blank
4. **Use PNG for quality** - JPEG for smaller files
5. **Save important captures** - Temp files are cleaned up automatically