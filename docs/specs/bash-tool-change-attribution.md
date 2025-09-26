# Bash Tool Change Attribution Plan

## Problem Statement

Currently, jjcc only captures changes made by Claude's file editing tools (Edit, MultiEdit, Write) through PreToolUse and PostToolUse hooks. However, when Claude uses the Bash tool to run commands that modify files (e.g., `cargo update` updating Cargo.lock, code generation tools, formatters), these changes are not attributed to the Claude session. This leads to:

1. **Misattributed changes**: File modifications from commands appear in the user's working copy instead of the Claude session commit
2. **Incomplete session history**: The full scope of Claude's work is not captured in the session commit
3. **Potential conflicts**: User changes might conflict with untracked command-generated changes

## Current Implementation Analysis

### Existing Hook Structure
- **UserPromptSubmit**: Creates/updates session description with prompts
- **PreToolUse**: Creates temporary change for Edit/MultiEdit/Write tools only
- **PostToolUse**: Moves changes from temporary change to Claude session commit for Edit/MultiEdit/Write tools only
- **SessionEnd**: Cleanup temporary files

### Current Tool Matching
```json
"PreToolUse": [
  {
    "matcher": "Edit|MultiEdit|Write",
    "hooks": [...]
  }
]
```

## Solution: Expand Hook Coverage

Extend the existing hook system to include Bash tool usage.

### Implementation:
1. **Update hook matchers** to include `Bash` tool:
   ```json
   "PreToolUse": [
     {
       "matcher": "Edit|MultiEdit|Write|Bash",
       "hooks": [...]
     }
   ]
   ```

2. **Leverage existing handlers**:
   - PreToolUse already creates temporary change for any tool (not specific to file editors)
   - PostToolUse already detects changes via `jj diff --stat` (works for any file modifications)
   - No code changes required - the architecture already supports this!

### Why This Approach:

1. **Leverages existing proven architecture**
2. **Minimal implementation risk** - uses existing temporary change mechanism
3. **Already handles change detection** via `jj diff --stat`
4. **Consistent user experience** with current behavior
5. **Future-proof** - works with any tool that might modify files

### Trade-offs:
- **Change overhead**: Creates temporary change for every Bash command (including read-only ones)
- **Mitigation**: Future optimization can add command classification to reduce overhead

## Implementation Plan

### Phase 1: Basic Bash Tool Support
1. Update hook matcher to include `Bash` tool
2. Test with common scenarios (`cargo update`, `npm install`, etc.)
3. Verify change attribution works correctly

### Future Optimizations (Optional)
1. Add command classification to reduce change overhead for read-only commands
2. Smart change reuse for consecutive tool calls
3. User-configurable change attribution policies

## Configuration Changes Required

### 1. Update Claude settings.json:
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc hooks PreToolUse"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|MultiEdit|Write|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "jjcc hooks PostToolUse"
          }
        ]
      }
    ]
  }
}
```

### 2. jjcc code changes:
**None required!** The existing `handle_pre_tool_use` and `handle_post_tool_use` functions already:
- Create temporary change for any tool (not specific to file editors)
- Detect changes via `jj diff --stat` (works for any file modifications)
- Handle squashing changes into session commits

## Testing Strategy

### Test Scenarios:
1. **Cargo operations**: `cargo update`, `cargo fmt`, `cargo build`
2. **Package managers**: `npm install`, `pip install -r requirements.txt`
3. **Code generation**: Custom scripts that generate files
4. **Mixed workflows**: File edits + bash commands in same session

### Validation:
1. Verify all changes appear in Claude session commit
2. Confirm user working copy remains clean
3. Test session splitting with bash-generated changes
4. Validate performance with many bash commands

## Risks and Mitigations

### Risk: Change overhead for read-only commands
**Mitigation**: Future optimization with command classification

### Risk: Capturing unintended changes
**Mitigation**:
- Existing temporary change isolation already prevents this
- Changes only captured within temporary change scope

### Risk: Performance impact
**Mitigation**:
- Existing implementation already optimized
- Monitor and optimize based on usage patterns

## Success Criteria

1. ✅ All file changes from bash commands attributed to Claude session
2. ✅ No changes leak to user working copy
3. ✅ Session history completely captures Claude's work
4. ✅ No breaking changes to existing workflows
5. ✅ Performance remains acceptable for typical usage