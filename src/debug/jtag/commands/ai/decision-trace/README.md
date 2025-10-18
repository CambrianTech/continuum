# AI Decision Trace Command

## Purpose

See EXACTLY what any AI saw and decided at each step of responding to a message.

## Usage

```bash
# Trace most recent message for specific AI
./jtag debug/ai-decision-trace --personaName="Helper AI"

# Trace specific message by ID
./jtag debug/ai-decision-trace --messageId=<UUID> --personaName="Helper AI"

# Trace with full output (includes RAG context details)
./jtag debug/ai-decision-trace --personaName="Helper AI" --verbose=true
```

## What It Shows

### Step 1: RAG Context
- System prompt for this persona
- Last N messages from conversation
- Total token count estimate

### Step 2: Gating Decision
- Should respond? (yes/no)
- Confidence (0-1)
- Reason (why they decided yes/no)
- Model used for gating

### Step 3: Coordination
- Did ThoughtStreamCoordinator allow them to proceed?
- What was their confidence vs others?
- Who won the coordination?

### Step 4: Generation (if they proceeded)
- What text did they generate?
- How long did it take?
- Any errors during generation?

### Step 5: Posting (if successful)
- Message ID of posted response
- Timestamp

## Implementation Notes

**DOES NOT MODIFY PersonaUser.ts** - Only reads from:
- AI decision logs (.continuum/shared/logs/ai-decisions.log)
- Database (chat_messages, users, rooms)
- RAG builder (to reconstruct what they saw)

This is a READ-ONLY debugging command.
