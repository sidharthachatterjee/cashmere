use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_unawaited_step_do_is_flagged() {
    // TypeScript code with unawaited step.do() call
    // No import needed - WorkflowStep comes from @cloudflare/workers-types ambient declarations
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

// ============================================================================
// nested-step rule tests
// ============================================================================

#[test]
fn test_nested_step_do_in_step_do_is_flagged() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        await step.do('inner', async () => {
            return { done: true };
        });
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("nested inside"),
        "Expected nested step error\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_nested_step_sleep_in_step_do_is_flagged() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        await step.sleep('wait', '1 second');
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("`step.sleep` is nested inside `step.do`"),
        "Expected nested step.sleep error\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_nested_step_wait_for_event_in_step_do_is_flagged() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        await step.waitForEvent('event', { timeout: '1 minute' });
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("`step.waitForEvent` is nested inside `step.do`"),
        "Expected nested step.waitForEvent error\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_nested_step_in_conditional_is_flagged() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        if (someCondition) {
            await step.do('inner', async () => {});
        }
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("nested inside"),
        "Expected nested step error even in conditional\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_nested_step_in_inline_function_is_flagged() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        const helper = async () => {
            await step.do('inner', async () => {});
        };
        await helper();
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("nested inside"),
        "Expected nested step error in inline function\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_deeply_nested_step_callbacks_flagged() {
    // Triple nesting: outer -> middle -> inner
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('outer', async () => {
        await step.do('middle', async () => {
            await step.do('inner', async () => {});
        });
    });
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".ts").unwrap();
    temp_file.write_all(typescript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have at least 2 nested-step errors (middle nested in outer, inner nested in middle)
    assert!(
        stdout.contains("[nested-step]"),
        "Expected [nested-step] rule name\nActual output:\n{}",
        stdout
    );
    // Count occurrences of "nested inside"
    let nested_count = stdout.matches("nested inside").count();
    assert!(
        nested_count >= 2,
        "Expected at least 2 nested step errors, found {}\nActual output:\n{}",
        nested_count,
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_sequential_steps_pass() {
    // Steps at the same level should not be flagged
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.do('first', async () => {
        return { done: true };
    });
    await step.sleep('wait', '1 second');
    await step.do('second', async () => {
        return { done: true };
    });
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
        "Expected no issues for sequential steps\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_step_sleep_outside_callback_passes() {
    let typescript_code = r#"
async function workflow(step: WorkflowStep) {
    await step.sleep('wait', '1 second');
    await step.waitForEvent('event', { timeout: '1 minute' });
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
        "Expected no issues for steps outside callbacks\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

// ============================================================================
// Semantic-based detection tests (false positive prevention)
// ============================================================================

#[test]
fn test_unrelated_step_object_not_flagged() {
    // This should NOT be flagged - "step" is just a regular object, not a WorkflowStep
    let typescript_code = r#"
const step = {
    do: async (name: string, fn: () => void) => { fn(); },
    sleep: async (name: string, duration: string) => {}
};

async function someFunction() {
    // These should NOT trigger errors - step is not a WorkflowStep
    step.do('task', async () => {});
    step.sleep('wait', '1 second');
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
        "Expected no issues for unrelated 'step' object\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_javascript_workflow_entrypoint_class_detected() {
    // JavaScript file (no type annotations) with class extending WorkflowEntrypoint
    // The 2nd parameter of run() should be inferred as WorkflowStep
    let javascript_code = r#"
import { WorkflowEntrypoint } from "cloudflare:workers";

export class MyWorkflow extends WorkflowEntrypoint {
    async run(event, step) {
        // This should be flagged - step.do() is not awaited
        step.do('task', async () => {});
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".js").unwrap();
    temp_file.write_all(javascript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("`step.do` must be awaited."),
        "Expected error for unawaited step.do() in JS workflow\nActual output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[await-step]"),
        "Expected [await-step] rule name\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input JavaScript ===");
    println!("{}", javascript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_javascript_workflow_entrypoint_awaited_passes() {
    // JavaScript file where step calls are properly awaited
    let javascript_code = r#"
import { WorkflowEntrypoint } from "cloudflare:workers";

export class MyWorkflow extends WorkflowEntrypoint {
    async run(event, step) {
        await step.do('task', async () => {});
        await step.sleep('wait', '1 second');
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".js").unwrap();
    temp_file.write_all(javascript_code.as_bytes()).unwrap();
    let temp_path = temp_file.path().to_str().unwrap();

    let mut cmd = Command::cargo_bin("cashmere").unwrap();
    let output = cmd.arg(temp_path).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No issues found"),
        "Expected no issues for properly awaited JS workflow\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input JavaScript ===");
    println!("{}", javascript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_different_type_name_not_flagged() {
    // If the type annotation is not literally "WorkflowStep", it should NOT be flagged
    // This tests that we only detect the specific type name
    let typescript_code = r#"
type WS = {
    do(name: string, fn: () => void): Promise<void>;
    sleep(name: string, duration: string): Promise<void>;
};

async function workflow(step: WS) {
    // This should NOT be flagged - type is "WS", not "WorkflowStep"
    step.do('task', async () => {});
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
        "Expected no issues when type is not 'WorkflowStep'\nActual output:\n{}",
        stdout
    );
    assert!(output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}

#[test]
fn test_workflow_step_type_always_detected() {
    // Any parameter typed as "WorkflowStep" should be detected, regardless of where the type is defined
    // The linter relies on the type name, assuming it comes from @cloudflare/workers-types
    let typescript_code = r#"
// Even with a local interface, if it's named WorkflowStep, it will be detected
// This is intentional - we trust that "WorkflowStep" means the Cloudflare Workflows type
interface WorkflowStep {
    do(name: string, fn: () => void): Promise<void>;
    sleep(name: string, duration: string): Promise<void>;
}

async function workflow(step: WorkflowStep) {
    // This WILL be flagged because the type is named "WorkflowStep"
    step.do('task', async () => {});
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
        "Expected error for unawaited step.do() with WorkflowStep type\nActual output:\n{}",
        stdout
    );
    assert!(!output.status.success());

    println!("=== Input TypeScript ===");
    println!("{}", typescript_code);
    println!("=== Actual Output ===");
    println!("{}", stdout);
}
