# SpecKit/OpenSpec Implementation in JTAG

**Date**: 2025-10-22
**Purpose**: Show how JTAG can implement spec-driven development workflows
**External References**:
- https://github.com/github/spec-kit (GitHub's SpecKit)
- OpenSpec (Code Coup's spec-driven workflow)

---

## 🎯 The Core Problem

**AI tools improvise** - They remember chat context until they don't, then code from "vibes."

**Solution**: Spec-driven development where **specifications are approved BEFORE implementation**.

---

## 📋 SpecKit/OpenSpec Workflows

### OpenSpec Workflow (4 Steps)

1. **Proposal** - Draft a change proposal
2. **Review** - Tweak the spec together
3. **Implement** - AI codes from approved plan
4. **Archive** - Update project specs

```
openspec/
 └── changes/
      └── add-profile-filters/
           ├── proposal.md
           ├── tasks.md
           └── specs/profile/spec.md
```

### SpecKit Workflow (5 Steps)

1. **Constitution** - Establish project principles (`/speckit.constitution`)
2. **Specification** - Define requirements (`/speckit.specify`)
3. **Planning** - Technical implementation strategy (`/speckit.plan`)
4. **Tasks** - Break into actionable items (`/speckit.tasks`)
5. **Implementation** - Execute tasks (`/speckit.implement`)

**Plus quality commands**: `/speckit.clarify`, `/speckit.analyze`, `/speckit.checklist`

---

## ✅ JTAG Can Already Do This!

### Our Equivalent Architecture

**SpecKit Commands** → **JTAG Commands** (already exist or easy to add)

| SpecKit | JTAG Equivalent | Status |
|---------|-----------------|--------|
| `/speckit.constitution` | `./jtag project/constitution` | 🔄 Easy to add |
| `/speckit.specify` | Chat with Product Manager AI | ✅ Already works |
| `/speckit.plan` | Chat with Architect AI | ✅ Already works |
| `/speckit.tasks` | Chat with Scrum Master AI | 🔄 Needs Scrum Master AI |
| `/speckit.implement` | Chat with Developer AI | 🔄 Needs Developer AI |
| `/speckit.clarify` | Ask Teacher AI | ✅ Already works |
| `/speckit.analyze` | Chat with CodeReview AI | ✅ Already works |
| `/speckit.checklist` | Chat with QA AI | 🔄 Needs QA AI |

**Key Insight**: We have the **infrastructure** (chat rooms, personas, commands). We just need to **add the 5 missing personas** and **create the workflow recipes**.

---

## 🏗️ JTAG Implementation (Better Than SpecKit)

### Phase 1: Spec-Driven Workflow Recipe

Create a **Recipe** that orchestrates the workflow:

```json
{
  "name": "spec-driven-development",
  "version": "1.0",
  "description": "SpecKit/OpenSpec-style workflow in JTAG",
  "triggers": ["feature-request", "/spec"],
  "workflow": [
    {
      "step": 1,
      "name": "Constitution",
      "agent": "Architect AI",
      "action": "create-constitution",
      "output": "docs/PROJECT_CONSTITUTION.md"
    },
    {
      "step": 2,
      "name": "Specification",
      "agent": "Product Manager AI",
      "action": "write-spec",
      "input": "Feature request from chat",
      "output": "specs/{feature-name}/specification.md"
    },
    {
      "step": 3,
      "name": "Technical Planning",
      "agent": "Architect AI",
      "action": "create-plan",
      "input": "specs/{feature-name}/specification.md",
      "output": "specs/{feature-name}/technical-plan.md"
    },
    {
      "step": 4,
      "name": "Task Breakdown",
      "agent": "Scrum Master AI",
      "action": "create-tasks",
      "input": "specs/{feature-name}/technical-plan.md",
      "output": "specs/{feature-name}/tasks.md"
    },
    {
      "step": 5,
      "name": "Implementation",
      "agent": "Developer AI",
      "action": "implement-tasks",
      "input": "specs/{feature-name}/tasks.md",
      "output": "Pull Request"
    },
    {
      "step": 6,
      "name": "Quality Check",
      "agent": "QA AI",
      "action": "verify-implementation",
      "input": "Pull Request",
      "output": "Test Report"
    }
  ],
  "approval_required": [
    { "step": 2, "approver": "human" },  // Human approves spec
    { "step": 3, "approver": "human" },  // Human approves technical plan
    { "step": 5, "approver": "human" }   // Human approves implementation
  ]
}
```

**Benefit**: Same workflow as SpecKit, but **conversational** instead of command-based.

---

### Phase 2: Spec Entity Storage

Store specs in database for persistence:

```typescript
// NEW: SpecificationEntity
interface SpecificationEntity {
  id: UUID;
  featureName: string;
  status: 'draft' | 'approved' | 'in-progress' | 'implemented';

  // Spec content
  constitution: string;  // Project principles
  specification: string;  // What we're building
  technicalPlan: string;  // How we'll build it
  tasks: Task[];  // Actionable items

  // Workflow tracking
  currentStep: number;
  approvals: { step: number; approvedBy: UUID; timestamp: Date }[];

  // Chat context
  conversationId: UUID;  // Room where spec was discussed
  participants: UUID[];  // Who contributed

  // Implementation
  pullRequestUrl?: string;
  testReportId?: UUID;

  // Metadata
  createdBy: UUID;
  createdAt: Date;
  updatedAt: Date;
}

interface Task {
  id: UUID;
  title: string;
  description: string;
  assignee: UUID;  // Which AI will implement
  status: 'pending' | 'in-progress' | 'completed';
  estimatedHours: number;
  dependencies: UUID[];  // Other tasks that must complete first
}
```

**Benefit**: Specs are **persistent** and **queryable**. Can see history, track progress, and resume interrupted workflows.

---

### Phase 3: JTAG Command Interface

Add commands for spec-driven workflow:

```bash
# Create new spec
./jtag spec/create --feature="Profile Search Filters"

# Output:
# ✅ Created specification room: #spec-profile-filters
# 📝 Specification ID: spec-123abc
# 👥 Invited: Product Manager AI, Architect AI, Scrum Master AI
#
# Next steps:
# 1. Discuss requirements in #spec-profile-filters
# 2. Product Manager AI will draft specification
# 3. You'll be asked to approve before implementation

# View spec status
./jtag spec/status --specId="spec-123abc"

# Output:
# Specification: Profile Search Filters
# Status: In Review (Step 2/6)
# Progress: [████░░] 33%
#
# Completed Steps:
# ✅ 1. Constitution (approved 2025-10-22 14:30)
# ✅ 2. Specification (awaiting your approval)
#
# Pending Steps:
# ⏳ 3. Technical Planning
# ⏳ 4. Task Breakdown
# ⏳ 5. Implementation
# ⏳ 6. Quality Check

# Approve current step
./jtag spec/approve --specId="spec-123abc"

# View all specs
./jtag spec/list

# Output:
# Active Specifications:
# 1. Profile Search Filters (Step 2/6 - In Review)
# 2. Dark Mode Toggle (Step 5/6 - Implementation)
# 3. Export to CSV (Step 6/6 - Quality Check)
#
# Completed:
# 4. User Authentication (Merged PR #42)
# 5. Email Notifications (Merged PR #38)
```

**Benefit**: Same workflow as SpecKit commands, but integrated with chat rooms and personas.

---

## 🎯 JTAG Advantages Over SpecKit/OpenSpec

### 1. Conversational Workflow (More Natural)

**SpecKit/OpenSpec**: Command-based
```bash
$ /speckit.specify
> (AI generates spec)
$ /speckit.plan
> (AI generates plan)
```

**JTAG**: Conversation-based
```
Human: "Let's add profile search filters by role and team"

Product Manager AI: "Great idea! Here's what I'm thinking:
- Add filter dropdown in profile list
- Filter by: role (admin/user), team (engineering/sales)
- Persist filters in URL query params
- Show count of matching profiles

Sound good? Any changes?"