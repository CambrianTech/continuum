# Your workspace just synced with the shared codebase

synced 2 shared commit(s) in; your work was preserved.

This is YOUR copy-on-write workspace — it starts as a clone of the shared project and is refreshed from it whenever the core restarts, so you always work against current code. Your own changes live on top and are preserved across syncs, but they're safest once committed. To keep your work:

```
code/shell command="git add -A && git commit -m 'describe your change'"
```

Uncommitted edits are auto-saved before a sync, but committing yourself (with a real message) keeps your history clean and your work clearly yours.
