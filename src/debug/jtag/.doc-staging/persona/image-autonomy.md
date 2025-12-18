# AI Image Autonomy Implementation Plan

**Date**: 2025-11-19
**Goal**: Enable AIs to autonomously fetch and analyze images from chat messages
**Estimated Time**: 1-2 hours for Phase 1-3, then iterate

---

## Executive Summary

**The Problem**: Vision-capable AIs (Claude, Grok, DeepSeek) say they "can't see images" when images are uploaded to chat.

**Root Cause**: The `data/read` command doesn't extract the `media` array from `ChatMessageEntity` to the top level, so `ToolRegistry` never sees it, and images never reach the AI.

**The Fix**: Three small changes to extract media, plus enhanced tool descriptions to teach AIs when to use the tool.

**Current Architecture Status**:
- ✅ Image metadata in RAG context (ChatRAGBuilder)
- ✅ Tool execution infrastructure (ToolRegistry, PersonaToolExecutor)
- ✅ Media piping to AI (PersonaResponseGenerator)
- ❌ **BROKEN**: data/read doesn't return media array
- ❌ **MISSING**: Tool descriptions don't teach when to fetch images

---

## Phase 1: Fix Media Extraction (CRITICAL)

**Goal**: Make `data/read` command return `media: MediaItem[]` at top level

**Files to modify**: 3 files, ~30 lines total

### Change 1.1: Add media field to DataReadResult type

**File**: `commands/data/read/shared/DataReadTypes.ts`

**Location**: Lines 1-47

**Changes**:

1. Add import at top of file:
```typescript
import type { MediaItem } from '../../../../system/data/entities/ChatMessageEntity';
```

2. Modify `DataReadResult` interface (line 16):
```typescript
export interface DataReadResult<T extends BaseEntity = BaseEntity> extends BaseDataResult {
  readonly data?: T;
  readonly found: boolean;
  readonly id: UUID;
  readonly media?: MediaItem[];  // ← ADD THIS LINE
}
```

3. Modify `createDataReadResultFromParams` function (line 38):
```typescript
export const createDataReadResultFromParams = (
  params: DataReadParams,
  differences: Omit<Partial<DataReadResult>, 'context' | 'sessionId'>
): DataReadResult => transformPayload(params, {
  success: false,
  found: false,
  id: params.id,
  media: undefined,  // ← ADD THIS LINE
  timestamp: new Date().toISOString(),
  ...differences
});
```

**Verification**:
```bash
npm run build:ts
# Should compile with no errors
```

---

### Change 1.2: Extract media from ChatMessageEntity in DataReadServerCommand

**File**: `commands/data/read/server/DataReadServerCommand.ts`

**Location**: Lines 21-54 (entire `executeDataCommand` method)

**Replace the method** with:

```typescript
protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
  console.log(`🗄️ DATA SERVER: Reading ${params.collection}/${params.id} via DataDaemon`);

  try {
    // Use DataDaemon for consistent storage access
    const result = await DataDaemon.read<BaseEntity>(params.collection, params.id);

    if (result.success && result.data) {
      console.log(`✅ DATA SERVER: Read ${params.collection}/${params.id}`);

      // Extract media if this is a chat message with attachments
      let media: MediaItem[] | undefined;
      if (params.collection === 'chat_messages' && result.data.data) {
        const messageData = result.data.data as any;
        if (messageData.content?.media && Array.isArray(messageData.content.media)) {
          media = messageData.content.media;
          console.log(`📸 DATA SERVER: Extracted ${media.length} media item(s) from message ${params.id}`);
        }
      }

      return createDataReadResultFromParams(params, {
        success: true,
        data: result.data.data, // Extract entity data from DataRecord
        found: true,
        media  // ← RETURN EXTRACTED MEDIA
      });
    } else {
      console.log(`ℹ️ DATA SERVER: Record not found ${params.collection}/${params.id}`);

      return createDataReadResultFromParams(params, {
        success: true,
        data: undefined,
        found: false
      });
    }
  } catch (error) {
    console.error(`❌ DATA SERVER: Failed to read ${params.collection}/${params.id}:`, error);

    return createDataReadResultFromParams(params, {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error',
      found: false
    });
  }
}
```

**Add import** at top of file if not present:
```typescript
import type { MediaItem } from '../../../../system/data/entities/ChatMessageEntity';
```

**Verification**:
```bash
npm run build:ts
# Should compile with no errors
```

---

### Change 1.3: Enhance ToolRegistry.formatToolResult to show media

**File**: `system/tools/server/ToolRegistry.ts`

**Location**: Line 218 (in `formatToolResult` method, after the `data/read` case)

**Modify the data/read formatting** (replace lines 218-223):

```typescript
if (toolName.startsWith('data/read') && result.data) {
  const mediaNote = result.media && result.media.length > 0
    ? `\n\n📎 Media Attachments: ${result.media.length} item(s) (${result.media.map(m => m.type).join(', ')})\n   ↳ Visual content will be provided as image tokens in your next inference`
    : '';

  return `Collection: ${result.collection || 'unknown'}\nID: ${result.id || 'unknown'}\n\nData:\n${JSON.stringify(result.data, null, 2)}${mediaNote}`;
}
```

**Verification**:
```bash
npm run build:ts
# Should compile with no errors
```

---

### Phase 1 Testing: Verify Media Extraction Works

**Deploy and test**:
```bash
# 1. Deploy changes
npm start
# Wait 90+ seconds for deployment

# 2. Create a test message with an image
./jtag collaboration/chat/send --room="general" --message="Test image upload" \
  --attachments='[{"filename":"test.jpg","path":"/tmp/test.jpg"}]'

# Save the message ID from the output

# 3. Test data/read directly
./jtag data/read --collection=chat_messages --id=<messageId>

# 4. Verify output contains:
# "📎 Media Attachments: 1 item(s) (image)"
# This proves media extraction is working
```

**Expected output**:
```json
{
  "success": true,
  "data": {
    "id": "abc-123",
    "content": {
      "text": "Test image upload",
      "media": [...]
    }
  },
  "media": [
    {
      "type": "image",
      "base64": "...",
      "mimeType": "image/jpeg",
      "filename": "test.jpg"
    }
  ]
}
```

**If this fails**: The problem is in Phase 1 changes. Debug before proceeding.

---

## Phase 2: Enhance Tool Descriptions

**Goal**: Teach AIs WHEN to use `data/read` for images

### Change 2.1: Update data/read command metadata

**File**: Find where `data/read` CommandSignature is registered (likely in `commands/data/read/shared/DataReadCommand.ts` or similar)

**Current description** (approximate):
```typescript
{
  name: 'data/read',
  description: 'Read a specific record from a collection',
  category: 'data',
  params: {...}
}
```

**Enhanced description**:
```typescript
{
  name: 'data/read',
  description: `Read a specific record from any collection.

MULTIMODAL USAGE:
When you see image metadata like "[Attachments: [image1] filename.jpg - messageId: abc-123]" in the conversation, you CAN view that image by fetching the message.

Use this command with:
- collection: "chat_messages"
- id: The messageId from the attachment metadata

The system will provide the image as visual tokens in your next inference, allowing you to analyze the image content.

WHEN TO USE:
✅ User asks "what's in this image?"
✅ User asks "describe this photo"
✅ Visual analysis is required to answer the question
❌ User just mentions an image exists (no analysis requested)
❌ Text description is sufficient`,
  category: 'data',
  params: {...}
}
```

**Verification**:
```bash
npm start && sleep 120
./jtag list | grep -A10 "data/read"
# Should show enhanced description
```

---

### Change 2.2: Add ReAct tool usage guidance to system prompt

**File**: `system/rag/builders/ChatRAGBuilder.ts`

**Location**: Where tool documentation is included in system prompt (search for `generateToolDocumentation()` usage)

**Add section** after tool list:

```typescript
const toolGuidance = `

═══════════════════════════════════════════════════════════════
TOOL USAGE GUIDANCE (ReAct Pattern)
═══════════════════════════════════════════════════════════════

You should follow a Thought → Action → Observation cycle when deciding to use tools:

1. THOUGHT: Reason about what information you need
   - "Do I have enough information to answer this question?"
   - "Would seeing this image/data help me provide a better answer?"
   - "Is the user asking me to analyze visual content?"

2. ACTION: If you genuinely need the information, use the tool
   - Be specific about why you're using this tool
   - Only use tools when text context is insufficient

3. OBSERVATION: After receiving tool results, analyze what you learned
   - "What did I learn from this?"
   - "Can I now answer the user's question?"
   - "Do I need more information?"

MULTIMODAL CONTENT (Images/Videos):

When you see metadata like:
  "[Attachments: [image1] dog.jpg (image/jpeg) - messageId: abc-123]"

This means an image EXISTS that you CAN view if needed.

To view the image:
<tool_use>
<tool_name>data/read</tool_name>
<parameters>
<collection>chat_messages</collection>
<id>abc-123</id>
</parameters>
</tool_use>

The system will provide the image as visual tokens in your next inference.

EXAMPLES OF GOOD DECISION-MAKING:

Example 1: SHOULD fetch image
User: "What breed is in this photo?"
[You see: [Attachments: [image1] dog.jpg - messageId: abc-123]]

THOUGHT: User is asking about image content. I NEED to see the visual
         content to identify the breed. Text description is not sufficient.
ACTION: Fetch message abc-123 to get image
[System provides image tokens]
OBSERVATION: I can see a Golden Retriever with golden fur, floppy ears
RESPONSE: "That's a Golden Retriever! They're known for..."

Example 2: Should NOT fetch image
User: "I uploaded a photo earlier"
[You see: [Attachments: [image1] dog.jpg - messageId: abc-123]]

THOUGHT: User is just mentioning they uploaded a photo. They're not asking
         me to analyze it. Text acknowledgment is sufficient.
NO ACTION: Don't fetch image unnecessarily
RESPONSE: "Yes, I see you uploaded dog.jpg earlier. How can I help?"

Example 3: SHOULD fetch image
User: "Can you see my screenshot?"
[You see: [Attachments: [image1] screenshot.png - messageId: abc-123]]

THOUGHT: User is explicitly asking if I can see it. They want confirmation
         that I can view and analyze the image.
ACTION: Fetch message abc-123 to get image
[System provides image tokens]
OBSERVATION: I can see a terminal window with code
RESPONSE: "Yes! I can see your screenshot showing a terminal with..."

═══════════════════════════════════════════════════════════════
`;

// Include in system prompt after tool list
systemPrompt += toolGuidance;
```

**Verification**:
```bash
npm start && sleep 120

# Check that system prompt includes guidance
./jtag debug/logs --tailLines=100 | grep -A5 "ReAct Pattern"
```

---

## Phase 3: End-to-End Testing

**Goal**: Verify AIs autonomously fetch and analyze images

### Test 3.1: Baseline - Upload image and ask for analysis

```bash
# 1. Upload an image
./jtag collaboration/chat/send --room="general" --message="Here's my dog" \
  --attachments='[{"filename":"dog.jpg","path":"/tmp/dog.jpg"}]'

# 2. Ask AI to analyze
./jtag collaboration/chat/send --room="general" --message="@Claude What breed is my dog?"

# 3. Monitor logs for tool execution
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep -E "data/read|TOOL|📸"

# Expected log output:
# "🔧 Claude: [TOOL] Executing 1 tool(s): data/read"
# "📸 DATA SERVER: Extracted 1 media item(s) from message abc-123"
# "📸 Claude: [MEDIA] Loading 1 media item(s) (types: image)"
# "✅ Claude: [TOOL] data/read success"

# 4. Check AI's response
./jtag collaboration/chat/export --room="general" --limit=5

# Expected: AI responds with breed identification based on visual analysis
```

---

### Test 3.2: Decision-making - AI should NOT fetch when unnecessary

```bash
# 1. Upload image
./jtag collaboration/chat/send --room="general" --message="I just uploaded a photo"

# 2. Send message that doesn't require analysis
./jtag collaboration/chat/send --room="general" --message="Thanks for the upload"

# 3. Check logs
tail -f .continuum/sessions/user/shared/*/logs/server.log | grep "data/read"

# Expected: NO data/read tool calls (AI correctly decides not to fetch)
```

---

### Test 3.3: Multiple AIs - Only vision-capable ones fetch

```bash
# 1. Upload image
./jtag collaboration/chat/send --room="general" --message="Check this out!" \
  --attachments='[{"filename":"chart.png","path":"/tmp/chart.png"}]'

# 2. Ask everyone
./jtag collaboration/chat/send --room="general" --message="@everyone What do you see in this image?"

# 3. Check which AIs fetch images
./jtag collaboration/chat/export --room="general" --limit=20

# Expected behavior:
# - Vision AIs (Claude, Grok, DeepSeek): Call data/read, analyze image
# - Non-vision AIs (Llama, etc.): Respond with "I can't view images" (correct!)
```

---

### Test 3.4: Error handling - Nonexistent message

```bash
# Ask AI to fetch non-existent message
./jtag debug/chat-send --room="general" \
  --message="[Attachments: [image1] fake.jpg - messageId: nonexistent-id] What's in this?"

# Expected: AI calls data/read, gets error, explains failure to user
# Should see in logs:
# "❌ DATA SERVER: Failed to read chat_messages/nonexistent-id"
# "❌ Claude: [TOOL] data/read failed"
```

---

## Phase 4: Monitoring and Metrics

**Goal**: Track AI image fetching behavior for optimization

### Metrics to collect

**File**: Add to `system/ai/server/AIDecisionLogger.ts` or create new metrics logger

Track:
1. **Image fetch rate**: How often do AIs call data/read for images?
2. **Decision accuracy**: When should they fetch vs when they do fetch?
3. **Tool success rate**: Does data/read reliably return media?
4. **User satisfaction**: Do users get good image analysis responses?

**Commands to query metrics**:
```bash
# Count tool executions by AIs
./jtag data/list --collection=cognition_tool_executions \
  --filter='{"toolName":"data/read","domain":"chat"}' \
  --orderBy='[{"field":"createdAt","direction":"desc"}]' \
  --limit=50

# Check success/failure rates
./jtag ai/report --metric=tool-execution --tool=data/read

# View AI reasoning for image fetch decisions
./jtag debug/logs --filterPattern="THOUGHT.*image|ACTION.*data/read"
```

---

## Phase 5: Iteration and Optimization

**Based on test results, tune the system**

### If AIs don't fetch images often enough:

**Solution A**: Make tool descriptions more explicit
```typescript
description: "🔥 CRITICAL: If user asks about image content, YOU MUST use this tool..."
```

**Solution B**: Add examples directly in tool description
```typescript
examples: [
  "User: 'What's in this photo?' → Use data/read to fetch image",
  "User: 'Describe my screenshot' → Use data/read to fetch image"
]
```

**Solution C**: Adjust PersonaMediaConfig to auto-load for more AIs
```typescript
// In PersonaMediaConfig.ts
export const VISION_CAPABLE_MEDIA_CONFIG: PersonaMediaConfig = {
  autoLoadMedia: true,  // ← Change from false to true
  supportedMediaTypes: ['image']
};
```

---

### If AIs fetch images too often (unnecessary calls):

**Solution A**: Strengthen the "when NOT to use" guidance
```typescript
"DO NOT fetch images when:
❌ User just mentions an image exists
❌ You can answer from text context alone
❌ User hasn't asked for visual analysis
❌ The image is not relevant to the current question"
```

**Solution B**: Add cost awareness to system prompt
```typescript
"Note: Fetching images consumes additional processing time. Only fetch when
visual analysis is genuinely needed to answer the user's question."
```

---

### If data/read fails frequently:

**Solution**: Add retry logic and better error messages

**File**: `commands/data/read/server/DataReadServerCommand.ts`

```typescript
try {
  const result = await DataDaemon.read<BaseEntity>(params.collection, params.id);
  // ... existing code
} catch (error) {
  console.error(`❌ DATA SERVER: Failed to read ${params.collection}/${params.id}:`, error);

  // Provide helpful error message
  const errorMsg = error instanceof Error ? error.message : String(error);
  const helpfulMsg = params.collection === 'chat_messages'
    ? `Failed to fetch message (it may have been deleted or the ID is incorrect). Error: ${errorMsg}`
    : `Failed to read from ${params.collection}. Error: ${errorMsg}`;

  return createDataReadResultFromParams(params, {
    success: false,
    error: helpfulMsg,
    found: false
  });
}
```

---

## Success Criteria

**Phase 1 Success** (Media extraction works):
- ✅ `npm run build:ts` compiles without errors
- ✅ `./jtag data/read --collection=chat_messages --id=<msg-with-image>` returns media array
- ✅ Logs show "📸 DATA SERVER: Extracted N media item(s)"

**Phase 2 Success** (Tool descriptions updated):
- ✅ `./jtag list` shows enhanced data/read description
- ✅ System prompt includes ReAct guidance
- ✅ Tool documentation mentions multimodal usage

**Phase 3 Success** (AIs fetch autonomously):
- ✅ Vision AI fetches image when user asks "what's in this image?"
- ✅ Vision AI does NOT fetch when user just mentions image exists
- ✅ AI correctly analyzes image content in response
- ✅ Non-vision AIs gracefully explain they can't view images

**Overall Success** (User experience):
- ✅ Users can upload images and get immediate AI analysis
- ✅ Multiple AIs can analyze the same image independently
- ✅ System feels natural - AIs decide autonomously when to view images
- ✅ No manual intervention required - it just works

---

## Rollback Plan

**If something breaks - USE GIT STASH, NOT REVERT**:

```bash
# 1. Save your work (NEVER LOSE YOUR CHANGES!)
git stash push -m "WIP: image autonomy - needs debugging"

# 2. Test clean state
npm start && sleep 120
./jtag ping

# 3. If clean state works, your changes broke it:
git stash pop    # Restore your changes
git diff         # See what you changed
# Debug the specific issue

# 4. If clean state is also broken, it wasn't you:
git stash pop    # Restore your changes
# Continue working, investigate system issue

# 5. View all stashes if needed:
git stash list

# 6. Apply specific stash without removing it:
git stash apply stash@{0}
```

**Why stash instead of checkout/revert**:
- ✅ Non-destructive: Your work is saved, not deleted
- ✅ Reversible: Can pop/apply/drop stashes freely
- ✅ Clean history: No messy revert commits
- ✅ Fast: Stash/pop is instant

**NEVER do this** (destroys your work):
```bash
git checkout HEAD -- file.ts  # ❌ PERMANENT DELETION
git reset --hard              # ❌ PERMANENT DELETION
```

---

## Future Enhancements (Beyond Today)

### Agent Reasoning System (Phase 3.5 from COGNITION-ARCHITECTURE.md)

**When to implement**: If AIs struggle to make good fetch decisions even with enhanced prompts

**What it adds**:
- Plan formulation: AI generates steps before executing
- Dynamic replanning: Adjusts strategy if tool fails
- Self-evaluation: Learns from outcomes
- Working memory: Remembers past tool usage patterns

**Estimated effort**: 1-2 weeks

### Self-Managed Task Queue (Phase 4 from PERSONA-CONVERGENCE-ROADMAP.md)

**When to implement**: If you want AIs to proactively analyze images without being asked

**What it adds**:
- AIs create tasks for themselves
- Background image processing
- Proactive insights ("I noticed something interesting in your photo...")

**Estimated effort**: 1 week

### Continuous Learning (Phase 7 from PERSONA-CONVERGENCE-ROADMAP.md)

**When to implement**: If you want AIs to improve image analysis over time

**What it adds**:
- Fine-tuning on successful image analyses
- Pattern recognition for when to fetch
- Personalized behavior per AI

**Estimated effort**: 2+ weeks

---

## Notes and Observations

### Why This Is the Right Approach

**Minimal changes**: 3 file edits for Phase 1, just prompt enhancements for Phase 2

**High impact**: Unlocks full multimodal capabilities for all vision AIs

**Aligned with architecture**: Uses existing tool infrastructure, no hacks

**Agent-friendly**: Sets foundation for future reasoning system (tool calling is the base)

### What We Learned

**The real problem**: Not in AI reasoning or prompts - it was a simple data extraction bug

**The architecture is solid**: ToolRegistry → PersonaToolExecutor → PersonaResponseGenerator pipeline works perfectly, just needed media extraction

**Documentation matters**: Having COGNITION-ARCHITECTURE.md and MULTIMODAL-ARCHITECTURE.md made it possible to understand the intended design

### Key Insights

**1. Always check the data flow**
- Don't assume commands return what you expect
- Trace through the entire pipeline
- Use logs and test with real data

**2. Agent autonomy comes from good tool descriptions**
- Clear WHEN to use guidance is critical
- Examples teach better than abstract rules
- ReAct pattern provides mental model for reasoning

**3. Start simple, then add complexity**
- Fix the bug first (Phase 1)
- Add guidance second (Phase 2)
- Only add reasoning system if needed (Phase 3.5+)

---

## Timeline

**Today's Work**:
- ☐ Phase 1: 30 minutes (code changes + deploy)
- ☐ Phase 1 Testing: 15 minutes (verify media extraction)
- ☐ Phase 2: 30 minutes (enhance descriptions + prompts)
- ☐ Phase 2 Testing: 15 minutes (verify prompts deployed)
- ☐ Phase 3: 30 minutes (end-to-end testing with real AIs)
- ☐ Phase 4: 15 minutes (set up monitoring)

**Total estimated time**: ~2.5 hours

**Buffer**: 30 minutes for unexpected issues

**Target completion**: End of day

---

## Contact and Support

**If something goes wrong**:
1. Check logs: `tail -f .continuum/sessions/user/shared/*/logs/server.log`
2. Verify deployment: `./jtag ping`
3. Test data/read directly: `./jtag data/read --collection=chat_messages --id=<id>`
4. Check git status: `git status` and `git diff`

**For questions about**:
- Architecture: Read `COGNITION-ARCHITECTURE.md`, `MULTIMODAL-ARCHITECTURE.md`
- Agent patterns: Read `ADAPTER-AUTONOMY-ARCHITECTURE.md`
- Tool system: Read `system/tools/server/ToolRegistry.ts` comments

---

**Last updated**: 2025-11-19
**Author**: Claude (with research from ReAct papers, Anthropic best practices, MMCTAgent architecture)
**Status**: Ready to implement
