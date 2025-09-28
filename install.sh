#!/bin/bash
set -e

cargo install --path .

SETTINGS_FILE="$HOME/.claude/settings.json"

if [ ! -f "$SETTINGS_FILE" ]; then
    echo "Claude settings file not found at $SETTINGS_FILE"
    echo "Skipping settings update"
    exit 0
fi

echo "Updating Claude Code hooks in $SETTINGS_FILE..."

if command -v jq >/dev/null 2>&1; then
    # Use jq to update the hooks
    TMP_FILE=$(mktemp)
    jq '
        .hooks.UserPromptSubmit[0].hooks[0].command = "jjcc claude hooks UserPromptSubmit" |
        .hooks.PreToolUse[0].hooks[0].command = "jjcc claude hooks PreToolUse" |
        .hooks.PostToolUse[0].hooks[0].command = "jjcc claude hooks PostToolUse" |
        .hooks.SessionEnd[0].hooks[0].command = "jjcc claude hooks SessionEnd"
    ' "$SETTINGS_FILE" > "$TMP_FILE" && mv "$TMP_FILE" "$SETTINGS_FILE"
    echo "✓ Updated Claude Code hooks to use new command structure"
else
    echo "jq not found - manual update required"
    echo "Please update your ~/.claude/settings.json to use:"
    echo "  - jjcc claude hooks UserPromptSubmit"
    echo "  - jjcc claude hooks PreToolUse"
    echo "  - jjcc claude hooks PostToolUse"
    echo "  - jjcc claude hooks SessionEnd"
fi