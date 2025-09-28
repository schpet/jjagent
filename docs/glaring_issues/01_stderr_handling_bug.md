# Issue #1: stderr Handling Bug in run_jj_command

## Severity
Critical

## Location
src/main.rs:531-549

## Description
The `run_jj_command` function inherits stderr (sending it directly to the terminal) but then attempts to read from `output.stderr` for error messages. This results in `output.stderr` always being empty, producing unhelpful error messages like "jj command failed: " with no actual error information.

```rust
fn run_jj_command(args: &[&str]) -> Result<()> {
    let output = Command::new("jj")
        .args(args)
        .stderr(std::process::Stdio::inherit())  // stderr goes to terminal
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);  // always empty!
        anyhow::bail!("jj command failed: {}", stderr);
    }
    // ...
}
```

## Steps to Reproduce
1. Create a test that calls `run_jj_command` with arguments that will fail (e.g., invalid revision)
2. Mock or use a real jj command that produces stderr output
3. Observe that the error message returned is "jj command failed: " with no error details
4. The actual jj error will have been printed to terminal but lost for programmatic error handling

Example:
```rust
// This will fail but won't capture the useful error message
run_jj_command(&["edit", "nonexistent_change_id_xyz"])?;
```

## Acceptance Criteria
- [ ] When `run_jj_command` fails, the returned error includes the actual stderr output from jj
- [ ] Error messages are captured programmatically and not just displayed to terminal
- [ ] Tests verify that stderr content is included in error messages
- [ ] Either:
  - Option A: Remove `.stderr(std::process::Stdio::inherit())` to capture stderr in `output.stderr`
  - Option B: Remove the attempt to read `output.stderr` and rely on inherited stderr (but this makes testing harder)

## Recommended Fix
Remove the `.stderr(std::process::Stdio::inherit())` line to allow capturing stderr:

```rust
fn run_jj_command(args: &[&str]) -> Result<()> {
    let output = Command::new("jj")
        .args(args)
        .output()
        .context("Failed to run jj command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj command failed: {}", stderr);
    }

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}
```