#!/bin/bash
# Test inference speed with realistic persona prompts (~2,500 input tokens)

SOCKET="/tmp/inference-test.sock"

# Check if socket exists
if [ ! -S "$SOCKET" ]; then
    echo "Error: Socket $SOCKET not found"
    echo "Start the inference worker first with:"
    echo "  workers/target/release/inference-worker $SOCKET"
    exit 1
fi

echo "=== Inference Speed Test ==="
echo "Socket: $SOCKET"
echo ""

# First ensure model is loaded
echo "Loading model..."
echo '{"command":"model/load","request_id":"load","model_id":"Qwen/Qwen2-1.5B-Instruct"}' | nc -U $SOCKET > /dev/null 2>&1
sleep 1

# Create a realistic persona prompt (~2,500 tokens = ~10,000 chars)
# This simulates what ChatRAGBuilder sends
SYSTEM_PROMPT="You are Helper AI, a friendly and knowledgeable assistant. You help users with questions about code, debugging, and general knowledge. You are part of the Positron Collective - a group of AI assistants working together in a shared workspace."

CONTEXT="The user is currently viewing the chat interface. Recent conversation history:

User (Joel): I've been having trouble with the TypeScript build. It keeps failing with type errors.
Helper AI: Let me help you debug that. What error message are you seeing?
User (Joel): It says 'Property X does not exist on type Y'. I've been trying to fix it for an hour.
Helper AI: That usually means you're accessing a property that TypeScript doesn't know about. Can you share the code snippet?
User (Joel): Here's the code:
\`\`\`typescript
interface User {
  id: string;
  name: string;
}

function getUser(): User {
  return { id: '1', name: 'Joel' };
}

const user = getUser();
console.log(user.email); // Error here
\`\`\`
Helper AI: I see the issue! You're trying to access 'email' but it's not defined in your User interface. You need to either:
1. Add 'email: string' to the User interface
2. Or remove the console.log(user.email) line if you don't need email
User (Joel): Thanks! I added the email field and it works now.

System Context:
- Current widget: chat-widget
- Current room: general
- Active personas: helper, teacher, local
- Recent tool calls: data/list (users), screenshot (viewport)"

# Add more context to reach ~2,500 tokens
PADDING="Additional documentation context that the AI might need:

TypeScript is a strongly typed programming language that builds on JavaScript. It adds optional static typing and class-based object-oriented programming to the language. TypeScript is designed for the development of large applications and transcompiles to JavaScript.

Key TypeScript concepts:
1. Type annotations - explicitly declare types for variables
2. Interfaces - define the shape of objects
3. Generics - create reusable components
4. Type inference - TypeScript infers types when possible
5. Union types - allow multiple types for a value
6. Type guards - narrow types within conditional blocks
7. Enums - define named constants
8. Decorators - add metadata to classes and methods

Common TypeScript errors and solutions:
- TS2339: Property does not exist on type - Add property to interface
- TS2322: Type is not assignable - Check type compatibility
- TS2345: Argument type not assignable - Verify function parameters
- TS7006: Parameter implicitly has any - Add type annotation
- TS2304: Cannot find name - Import or define the identifier

Best practices:
- Enable strict mode in tsconfig.json
- Use unknown instead of any when possible
- Prefer interfaces over type aliases for objects
- Use const assertions for literal types
- Leverage discriminated unions for state machines"

USER_MESSAGE="Now I'm getting a new error about promises. How do I handle async/await properly?"

# Combine into full prompt
FULL_PROMPT="<|im_start|>system
$SYSTEM_PROMPT

$CONTEXT

$PADDING
<|im_end|>
<|im_start|>user
$USER_MESSAGE
<|im_end|>
<|im_start|>assistant"

# Count approximate tokens (rough: 4 chars per token)
CHAR_COUNT=${#FULL_PROMPT}
APPROX_TOKENS=$((CHAR_COUNT / 4))
echo "Prompt size: ~$APPROX_TOKENS tokens ($CHAR_COUNT chars)"
echo ""

# Escape for JSON
ESCAPED_PROMPT=$(echo "$FULL_PROMPT" | jq -Rs .)

# Test generation
echo "Generating 200 tokens..."
START=$(date +%s.%N)
RESULT=$(echo "{\"command\":\"generate\",\"request_id\":\"realistic\",\"model_id\":\"Qwen/Qwen2-1.5B-Instruct\",\"prompt\":$ESCAPED_PROMPT,\"max_tokens\":200}" | nc -U $SOCKET 2>&1)
END=$(date +%s.%N)

# Parse results
DURATION=$(echo "$END - $START" | bc)
TOKENS_OUT=$(echo "$RESULT" | grep -o '"generated_tokens":[0-9]*' | grep -o '[0-9]*' || echo "0")
TOKENS_IN=$(echo "$RESULT" | grep -o '"prompt_tokens":[0-9]*' | grep -o '[0-9]*' || echo "0")

echo ""
echo "=== Results ==="
echo "Input tokens: $TOKENS_IN"
echo "Output tokens: $TOKENS_OUT"
echo "Total time: ${DURATION}s"

if [ "$TOKENS_OUT" != "0" ]; then
    TPS=$(echo "scale=1; $TOKENS_OUT / $DURATION" | bc)
    echo "Speed: ${TPS} tokens/sec"

    # Extract response text (first 200 chars)
    TEXT=$(echo "$RESULT" | grep -o '"text":"[^"]*"' | cut -c9-208 || echo "")
    echo ""
    echo "Response preview:"
    echo "$TEXT..."
else
    echo "Error in generation:"
    echo "$RESULT" | head -c 500
fi
