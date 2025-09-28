# Issue #4: Silent stdin Failure

## Severity
Medium

## Location
- src/main.rs:263-272 (in `handle_pre_tool_use`)
- src/main.rs:383-392 (in `handle_post_tool_use`)

## Description
The code spawns processes with `--stdin` flag but silently skips writing to stdin if `child.stdin` is `None`. This can cause the process to hang indefinitely waiting for input that never comes, or fail in unexpected ways.

```rust
let mut child = Command::new("jj")
    .args(["describe", "--stdin"])
    .stdin(std::process::Stdio::piped())
    .spawn()?;

if let Some(stdin) = child.stdin.as_mut() {
    use std::io::Write;
    stdin.write_all(message.as_bytes())?;
}
child.wait()?;  // If stdin was None, jj is waiting for input that will never come
```

While `Stdio::piped()` should ensure stdin is available, defensive programming requires handling the None case explicitly.

## Steps to Reproduce
1. Create a scenario where `child.stdin` could be None:
   - Mock the Command to return a child with stdin = None
   - Test edge cases where pipe creation might fail
2. Trigger either `handle_pre_tool_use` or `handle_post_tool_use`
3. Observe that:
   - The message is silently not written to stdin
   - `child.wait()` hangs indefinitely waiting for stdin input
   - Or the jj command fails with confusing error about missing input

Example test:
```rust
#[test]
fn test_stdin_none_handling() {
    // Mock Command where stdin is None
    // Current code: silently fails or hangs
    // Expected: return clear error
}
```

## Acceptance Criteria
- [ ] Code explicitly handles the case where `child.stdin` is `None`
- [ ] Returns a clear error message explaining the stdin pipe failure
- [ ] No silent failures or hangs
- [ ] Tests verify error handling for stdin failures
- [ ] Consider: ensure stdin pipe is properly set up before writing

## Recommended Fix
```rust
let mut child = Command::new("jj")
    .args(["describe", "--stdin"])
    .stdin(std::process::Stdio::piped())
    .spawn()?;

let stdin = child.stdin.as_mut()
    .context("Failed to open stdin pipe for jj describe command")?;

use std::io::Write;
stdin.write_all(message.as_bytes())
    .context("Failed to write message to jj describe stdin")?;

// Explicitly drop stdin to close the pipe
drop(stdin);

child.wait()
    .context("Failed to wait for jj describe command")?;
```

Note: Explicitly dropping stdin after writing is good practice to close the pipe and signal EOF to the child process.