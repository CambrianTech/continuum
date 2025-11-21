# Message Composer Widget - Rich Multimodal Input

## Problem

Current chat input is a basic text field that doesn't support:
- ❌ Drag & drop images/files
- ❌ Visual file previews
- ❌ Markdown syntax highlighting
- ❌ Multi-file attachments like Slack/Discord
- ❌ Image placeholders in text ([Image #1])

## Solution: Dedicated Message Composer Widget

Create `<message-composer-widget>` that replaces the current text input.

### Component Structure

```
<message-composer-widget>
  ├── <composer-input-area>           # Rich text editor
  │   ├── Markdown syntax highlighting
  │   ├── ``` code block detection
  │   └── [Image #N] placeholder insertion
  │
  ├── <composer-attachments-preview>  # File previews
  │   ├── Image thumbnails with remove (X) button
  │   ├── File icons for documents/audio/video
  │   └── Upload progress indicators
  │
  ├── <composer-toolbar>              # Action buttons
  │   ├── 📎 Attach file button
  │   ├── 🖼️ Insert image button
  │   ├── 😊 Emoji picker button
  │   └── ➤ Send button
  │
  └── <composer-drop-zone>            # Drag & drop overlay
      └── Shows when dragging over chat OR composer
```

### Drag & Drop Behavior

**Two drop zones:**

1. **Composer drop zone** (always active)
   ```typescript
   // User drops on input area → attach to current message
   composer.addEventListener('drop', (e) => {
     const files = Array.from(e.dataTransfer.files);
     this.attachFiles(files);
   });
   ```

2. **Chat area drop zone** (global)
   ```typescript
   // User drops anywhere on chat → focus composer and attach
   chatWidget.addEventListener('drop', (e) => {
     if (e.target.closest('message-composer-widget')) return; // Already handled

     const files = Array.from(e.dataTransfer.files);
     this.composer.focus();
     this.composer.attachFiles(files);
   });
   ```

**Visual feedback:**
```css
.drop-zone-active {
  border: 3px dashed var(--accent-color);
  background: var(--drop-zone-bg);
  opacity: 0.9;
}

.drop-zone-active::after {
  content: "Drop images here";
  font-size: 24px;
  color: var(--accent-color);
}
```

### File Attachment Flow

```typescript
interface AttachedFile {
  id: string;              // Unique ID for this attachment
  file: File;              // Original File object
  type: MediaType;         // 'image' | 'audio' | 'video' | 'file'
  preview?: string;        // Data URL for preview
  uploadProgress?: number; // 0-100
}

class MessageComposerWidget {
  private attachedFiles: AttachedFile[] = [];

  async attachFiles(files: File[]): Promise<void> {
    for (const file of files) {
      const attachedFile: AttachedFile = {
        id: crypto.randomUUID(),
        file,
        type: this.detectMediaType(file),
        uploadProgress: 0
      };

      // Generate preview for images
      if (file.type.startsWith('image/')) {
        attachedFile.preview = await this.generateImagePreview(file);
      }

      this.attachedFiles.push(attachedFile);
      this.renderAttachmentPreview(attachedFile);

      // Insert placeholder in text
      const placeholder = `[Image #${this.attachedFiles.length}]`;
      this.insertTextAtCursor(placeholder);
    }
  }

  async sendMessage(): Promise<void> {
    const text = this.inputArea.value;

    // Convert File objects to MediaItems
    const media: MediaItem[] = await Promise.all(
      this.attachedFiles.map(async (attached) => ({
        id: attached.id,
        type: attached.type,
        base64: await this.fileToBase64(attached.file),
        filename: attached.file.name,
        mimeType: attached.file.type,
        size: attached.file.size,
        uploadedAt: Date.now(),
        uploadedBy: this.currentUserId
      }))
    );

    // Send via chat/send command
    await Commands.execute('chat/send', {
      roomId: this.currentRoomId,
      message: text,
      media
    });

    // Clear composer
    this.clearComposer();
  }
}
```

### Markdown with Image Placeholders

**Input:**
```
Check out these UI bugs:

[Image #1] - Submit button overlaps input
[Image #2] - Text is clipped

The CSS needs fixing!
```

**Rendered Message:**
```
Check out these UI bugs:

[Image thumbnail 1]
Submit button overlaps input

[Image thumbnail 2]
Text is clipped

The CSS needs fixing!
```

### Message Row Rendering

Update `BaseMessageRowWidget` to render media:

```typescript
// widgets/chat/shared/BaseMessageRowWidget.ts

renderMessageContent(content: MessageContent): HTMLElement {
  const container = document.createElement('div');
  container.className = 'message-content';

  // Replace [Image #N] placeholders with actual images
  let text = content.text;
  const imageRegex = /\[Image #(\d+)\]/g;

  const parts = text.split(imageRegex);
  let imageIndex = 0;

  for (let i = 0; i < parts.length; i++) {
    if (i % 2 === 0) {
      // Text part
      if (parts[i]) {
        const textNode = document.createElement('div');
        textNode.className = 'message-text';
        textNode.textContent = parts[i];
        container.appendChild(textNode);
      }
    } else {
      // Image placeholder - replace with actual image
      const imageNum = parseInt(parts[i]);
      const mediaItem = content.media?.[imageNum - 1];

      if (mediaItem) {
        const img = this.renderMediaItem(mediaItem);
        container.appendChild(img);
      }
    }
  }

  return container;
}

renderMediaItem(media: MediaItem): HTMLElement {
  const container = document.createElement('div');
  container.className = `media-item media-${media.type}`;

  if (media.type === 'image') {
    const img = document.createElement('img');
    img.src = media.url || `data:${media.mimeType};base64,${media.base64}`;
    img.alt = media.alt || media.description || 'Attached image';
    img.className = 'message-image';

    // Click to expand
    img.addEventListener('click', () => {
      this.openImageViewer(media);
    });

    container.appendChild(img);
  } else {
    // File/audio/video - show icon + filename
    const fileIcon = document.createElement('div');
    fileIcon.className = 'file-icon';
    fileIcon.textContent = this.getFileIcon(media.type);

    const fileName = document.createElement('span');
    fileName.className = 'file-name';
    fileName.textContent = media.filename || 'Untitled';

    container.appendChild(fileIcon);
    container.appendChild(fileName);
  }

  return container;
}
```

### CSS Structure

```css
/* Composer widget */
message-composer-widget {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-top: 1px solid var(--border-color);
}

/* Input area */
.composer-input-area {
  min-height: 60px;
  max-height: 300px;
  overflow-y: auto;
  padding: 8px 12px;
  border: 1px solid var(--input-border);
  border-radius: 8px;
  font-family: var(--font-mono);
  line-height: 1.5;
}

/* Syntax highlighting for code blocks */
.composer-input-area .code-block {
  background: var(--code-bg);
  color: var(--code-text);
  padding: 2px 4px;
  border-radius: 3px;
}

/* Attachment previews */
.composer-attachments-preview {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.attachment-preview {
  position: relative;
  width: 80px;
  height: 80px;
  border-radius: 8px;
  overflow: hidden;
  border: 2px solid var(--border-color);
}

.attachment-preview img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.attachment-preview .remove-btn {
  position: absolute;
  top: 4px;
  right: 4px;
  background: rgba(0,0,0,0.7);
  color: white;
  border: none;
  border-radius: 50%;
  width: 20px;
  height: 20px;
  cursor: pointer;
  font-size: 14px;
  line-height: 20px;
}

/* Message rendering */
.message-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.message-image {
  max-width: 400px;
  max-height: 300px;
  border-radius: 8px;
  cursor: pointer;
  transition: transform 0.2s;
}

.message-image:hover {
  transform: scale(1.02);
}

.media-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: var(--attachment-bg);
  border-radius: 8px;
  border: 1px solid var(--border-color);
}
```

## Implementation Plan

### Phase 1: Basic Composer Widget
- [ ] Create `message-composer-widget.ts`
- [ ] File attachment button → file picker
- [ ] Display file previews with remove buttons
- [ ] Send message with media array

### Phase 2: Drag & Drop
- [ ] Composer drop zone (always active)
- [ ] Chat area drop zone (global fallback)
- [ ] Visual feedback overlay
- [ ] Multi-file support

### Phase 3: Placeholder System
- [ ] Auto-insert [Image #N] on attach
- [ ] Parse placeholders in message text
- [ ] Replace with actual images in message rows

### Phase 4: Rich Input
- [ ] Markdown syntax highlighting
- [ ] Code block detection (```)
- [ ] Integration with MessageInputEnhancer

### Phase 5: Message Row Improvements
- [ ] Render media items inline
- [ ] Image lightbox viewer
- [ ] File download links
- [ ] Audio/video players

## Testing

```bash
# Test 1: File picker
./jtag screenshot --querySelector="message-composer-widget .attach-btn"
# Click button, select image, verify preview appears

# Test 2: Drag & drop
# Drag image onto composer → preview appears + [Image #1] inserted

# Test 3: Multi-file
# Attach 3 images → see 3 previews + [Image #1] [Image #2] [Image #3]

# Test 4: Send message
./jtag chat/send --room="general" --message="Test" --image="/path/to/image.png"
# Message appears with image inline

# Test 5: Message row rendering
./jtag screenshot --querySelector=".message-row:last-child"
# Verify image renders correctly in message
```

## Future Enhancements

- **Paste images** from clipboard
- **Emoji picker** integration
- **GIF search** (Giphy/Tenor)
- **Voice messages** (record audio)
- **Screen recording** clips
- **Code syntax highlighting** in messages
- **LaTeX math** rendering
- **Mermaid diagrams**
- **Interactive polls**
- **Message reactions** with images

---

**Status**: Design complete, ready for implementation
**Priority**: HIGH - Critical for multimodal AI interaction
