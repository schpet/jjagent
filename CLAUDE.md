# jjcc Development Guide

## Running Tests

Run all tests:
```bash
cargo test
```

Run tests quietly (less output):
```bash
cargo test --quiet
```

Run a specific test:
```bash
cargo test test_first_tool_use
```

Run tests with output displayed:
```bash
cargo test -- --nocapture
```


## manually verifying

feel free to make test jj repos in /tmp and run claude for a real test, note you can use the session id this way:

```bash
# Start the conversation and capture the session ID
initial_output=$(claude -p "This is the very first prompt" --output-format json)
SESSION_ID=$(echo "$initial_output" | jq -r '.session_id')

echo "Started session: $SESSION_ID"

# Now, for all future calls in your script, use that ID
claude -r "$SESSION_ID" -p "This is the second prompt"
claude -r "$SESSION_ID" -p "This is the third prompt"
```
