use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_unawaited_step_do_is_flagged() {
    // TypeScript code with unawaited step.do() call
    let typescript_code = r#"
export class MyWorkflow {
    async run(step: WorkflowStep) {
        // This should be flagged - step.do() is not awaited
        step.do('send-email', async () => {
            return { sent: true };
        });

        // This is fine - properly awaited
        await step.do('save-to-db', async () => {
            return { saved: true };
        });
    }
}
"#;

    // Expected output from the linter
    let expected_output = r#"
:5:9 - `step.do` must be awaited. Not awaiting creates a dangling Promise that can cause race conditions and swallowed errors. [await-step]

✗ Found 1 issue(s) in 1 file(s) checked
"#;

    // Create a temporary TypeScript file
    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    // Run the linter
    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The file path in output will vary, so we check the important parts
    assert!(
        stdout.contains(":5:9 - `step.do` must be awaited."),
        "Expected line 5, column 9 error for unawaited step.do()\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[await-step]"),
        "Expected [await-step] rule name in output\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Found 1 issue(s)"),
        "Expected exactly 1 issue\nActual output:\n{}",
        stdout
    );

    // Should exit with non-zero status when issues found
    assert!(!output.status.success());

    // Print for human readability when running with --nocapture
    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Expected Output (key parts) ==={}", expected_output);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_awaited_step_do_passes() {
    // TypeScript code where all step calls are properly awaited
    let typescript_code = r#"
export class MyWorkflow {
    async run(step: WorkflowStep) {
        await step.do('task-1', async () => {
            return { done: true };
        });

        const result = await step.do('task-2', async () => {
            return { value: 42 };
        });

        await step.sleep('wait', '1 hour');
    }
}
"#;

    // Expected output - no issues
    let expected_output = r#"
✓ No issues found (1 files checked)
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues for properly awaited code\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Expected Output ==={}", expected_output);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_unawaited_step_sleep_is_flagged() {
    // TypeScript code with unawaited step.sleep() call
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    // This should be flagged - step.sleep() is not awaited
    step.sleep('pause', '30 seconds');
}
"#;

    // Expected output
    let expected_output = r#"
:4:5 - `step.sleep` must be awaited. Not awaiting creates a dangling Promise that can cause race conditions and swallowed errors. [await-step]

✗ Found 1 issue(s) in 1 file(s) checked
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains(":4:5 - `step.sleep` must be awaited."),
        "Expected line 4, column 5 error for unawaited step.sleep()\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Found 1 issue(s)"),
        "Expected exactly 1 issue\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Expected Output (key parts) ==={}", expected_output);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_promise_awaited_later_passes() {
    // TypeScript code where step promise is assigned to variable and awaited later
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p = step.do('task-1', async () => {
        return { done: true };
    });
    // Some other code
    const x = 1 + 2;
    // Now await the promise
    await p;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues when step promise is awaited later\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_promises_awaited_with_promise_all_passes() {
    // TypeScript code where step promises are awaited via Promise.all
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p1 = step.do('task-1', async () => {
        return { done: true };
    });
    const p2 = step.do('task-2', async () => {
        return { done: true };
    });
    const p3 = step.sleep('pause', '1 second');

    await Promise.all([p1, p2, p3]);
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues when step promises are awaited via Promise.all\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_promises_awaited_with_promise_race_passes() {
    // TypeScript code where step promises are awaited via Promise.race
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p1 = step.do('task-1', async () => {
        return { done: true };
    });
    const p2 = step.do('task-2', async () => {
        return { done: true };
    });

    await Promise.race([p1, p2]);
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues when step promises are awaited via Promise.race\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_promises_awaited_with_promise_allsettled_passes() {
    // TypeScript code where step promises are awaited via Promise.allSettled
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p1 = step.do('task-1', async () => {
        return { done: true };
    });
    const p2 = step.do('task-2', async () => {
        return { done: true };
    });

    await Promise.allSettled([p1, p2]);
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues when step promises are awaited via Promise.allSettled\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_promise_assigned_but_never_awaited_is_flagged() {
    // TypeScript code where step promise is assigned but never awaited
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p = step.do('task-1', async () => {
        return { done: true };
    });
    // p is never awaited!
    const x = 1 + 2;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("`step.do` must be awaited."),
        "Expected error for step promise that is never awaited\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Found 1 issue(s)"),
        "Expected exactly 1 issue\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_calls_directly_in_promise_all_passes() {
    // TypeScript code where step calls are passed directly to Promise.all without variable assignment
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await Promise.all([
        step.sleep("blah", "3 minutes"),
        step.sleep("wait on something", "4 minutes"),
        step.waitForEvent("wait for human approval", {
            timeout: "5 minutes",
            type: "human_approval",
        }),
    ]);
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues when step calls are directly in Promise.all\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_partial_await_only_one_promise_awaited() {
    // TypeScript code where one step promise is awaited but another is not
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    const p1 = step.do('task-1', async () => {
        return { done: true };
    });
    const p2 = step.do('task-2', async () => {
        return { done: true };
    });
    // Only p1 is awaited
    await p1;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("`step.do` must be awaited."),
        "Expected error for p2 step promise that is never awaited\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Found 1 issue(s)"),
        "Expected exactly 1 issue (for p2)\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}
