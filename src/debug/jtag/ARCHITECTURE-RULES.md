# ARCHITECTURE RULES - MUST READ BEFORE CODING

**CRITICAL: Read ALL rules before writing ANY code in this system.**

## 🚨 **CARDINAL SINS - NEVER DO THESE:**

### **1. Type System Violations**
❌ **NEVER use `any` types** - Always use proper TypeScript interfaces
❌ **NEVER use `unknown` without extreme justification** - Import correct types instead
❌ **NEVER write loose, optional-chaining-heavy code** - Use strict typing

### **2. Environment Mixing (CRITICAL)**
❌ **NEVER put server/Node code in `/shared` directories**
❌ **NEVER put browser-specific code in `/shared` directories**
❌ **NEVER use `typeof window`, `typeof process` checks in shared code**
❌ **NEVER import server modules in browser code or vice versa**
❌ **NEVER use dynamic imports/requires** - Use static imports at top of file

### **3. Abstraction Violations**
❌ **NEVER bypass daemon/command patterns** - Use established abstractions
❌ **NEVER write inline conditional logic instead of using proper classes**
❌ **NEVER create switch statements for entity types** - Keep code generic

### **4. Entity System Violations (MOST CRITICAL)**
❌ **NEVER reference derived entity types (`UserEntity`, `ChatMessageEntity`, `RoomEntity`) in data layers**
❌ **NEVER hardcode collection names (`'users'`, `'rooms'`) in generic code**
❌ **NEVER write entity-specific logic in data/event systems**
❌ **NEVER create conditional statements based on entity types**

## ✅ **RUST-LIKE PRINCIPLES - ALWAYS FOLLOW:**

### **1. Strict Typing**
✅ **Use `<T extends BaseEntity>` for proper constraint inheritance**
✅ **Use `Partial<T>` for updates, not loose objects**
✅ **Use union types** - `'created' | 'updated' | 'deleted'` not strings
✅ **Use template literals** - `` `data:${Collection}:${Action}` `` for type safety
✅ **Use discriminated unions** for clean pattern matching

### **2. Generic Programming**
✅ **Data layer only knows `BaseEntity`** - reads `entity.collection` property
✅ **Event system uses `entity.collection`** - never hardcoded collection strings
✅ **Write code that works with ANY entity type automatically**
✅ **Use `BaseEntity.collection` to get collection name from entity**

### **3. Abstraction Layers**
✅ **Follow shared/browser/server pattern** - 85% shared logic
✅ **Use daemon/command architecture** for all system operations
✅ **Keep shared code environment-agnostic**
✅ **Build on existing patterns, don't reinvent**

### **4. Research First**
✅ **Study existing codebase before writing new code**
✅ **Look for existing patterns and utilities**
✅ **Extend existing interfaces, don't create new ones**
✅ **Ask "What already exists?" before coding**

## 🎯 **SPECIFIC SYSTEM RULES:**

### **Event System**
✅ **Server emits:** `Events.emit(\`data:${entity.collection}:created\`, entity)`
✅ **Browser subscribes:** `Events.subscribe('data:users')` (collection name allowed in client)
✅ **Data layer:** Only knows `BaseEntity`, never specific entity types
✅ **Event names:** Always derived from `entity.collection`, never hardcoded

### **Data Layer**
✅ **Generic:** Works with any entity extending `BaseEntity`
✅ **Collection source:** Always from `entity.collection` property
✅ **Storage:** Adapters handle collection→table mapping
✅ **Queries:** Use generic filtering, not entity-specific logic

### **Widget Layer**
✅ **Can know specific entity types** (UserEntity, ChatMessageEntity)
✅ **Can have entity-specific logic** and business rules
✅ **Interfaces with data layer generically** via BaseEntity
✅ **Handles type casting** from BaseEntity to specific types

## ⚠️ **COMPLEXITY WARNING SIGNS:**

### **When to Step Back:**
❌ **Generics nested 3+ levels deep** - Simplify the abstraction
❌ **Need `as any` to make types work** - Wrong approach, redesign
❌ **Interface has 10+ properties** - Break it down
❌ **Fighting TypeScript** - Redesign, don't force
❌ **Creating switch statements** - Use polymorphism instead
❌ **Hardcoding entity names** - Use generic patterns

### **Good Architecture Indicators:**
✅ **Adding new entity types requires zero code changes** in data layer
✅ **Event system works automatically** with new entities
✅ **No conditional logic** based on collection names
✅ **TypeScript compiles without warnings**
✅ **Code is self-documenting** through types

## 🔬 **DEVELOPMENT METHODOLOGY:**

### **Before Writing Code:**
1. **Research existing patterns** - What already exists?
2. **Identify abstraction level** - Data/Event/Widget layer?
3. **Check environment** - Shared/Browser/Server?
4. **Verify generic approach** - Works with ANY entity?
5. **Design types first** - Proper generics and constraints

### **Architecture Validation:**
- [ ] Works with BaseEntity only, no specific types
- [ ] Uses `entity.collection`, no hardcoded collections
- [ ] Environment-appropriate (shared/browser/server)
- [ ] Extends existing patterns, doesn't reinvent
- [ ] Adding new entity requires zero data layer changes

## 🎯 **SUCCESS CRITERIA:**

**The system is correctly architected when:**
1. **Adding `ProjectEntity` requires ZERO changes** to data/event systems
2. **Collection name comes from entity,** not hardcoded anywhere
3. **Data layer compiles without knowing** about UserEntity/ChatMessageEntity
4. **Event system works generically** for any entity type
5. **TypeScript enforces correctness** without `any` escape hatches

## 🔍 **VALIDATION TEST - THE SEARCH TEST:**

**✅ SUCCESS INDICATOR:**
```bash
# Search event/data code for specific entities - should find ZERO results
grep -r "UserEntity\|ChatMessageEntity\|RoomEntity" daemons/events-daemon/
grep -r "UserEntity\|ChatMessageEntity\|RoomEntity" daemons/data-daemon/
grep -r "UserEntity\|ChatMessageEntity\|RoomEntity" system/events/
grep -r "UserEntity\|ChatMessageEntity\|RoomEntity" commands/data/
```

**❌ CURRENT STATUS: VIOLATIONS FOUND**
- `commands/data/list/server/DataListServerCommand.ts` imports specific entities

**IMMEDIATE ACTION REQUIRED:**
1. **Remove specific entity imports** from all data commands
2. **Make data commands work generically** with BaseEntity only
3. **Ensure data commands use `entity.collection`** not hardcoded collections

**NOTE:** Documentation examples with specific entities are acceptable for illustration.

**If ANY results found = ARCHITECTURE VIOLATION**

**The event/data systems should be 100% generic - no specific entity references anywhere.**

---

**REMEMBER: Make the complex simple, not the simple complex.**

**The goal: Write code once, works with infinite entity types.**