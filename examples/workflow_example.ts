// Example Cloudflare Workflow with linting errors and correct patterns

export class ExampleWorkflow {
    async run(step: any) {
        // ❌ BAD: Unawaited step call
        step.do('send-email', async () => {
            return { sent: true };
        });

        // ✅ GOOD: Properly awaited
        await step.do('save-to-db', async () => {
            return { saved: true };
        });

        // ❌ BAD: Assigned but never awaited
        const promise = step.sleep('wait', '1 hour');
        console.log('This will execute before the sleep completes!');

        // ✅ GOOD: Assigned and awaited later
        const task1 = step.do('task-1', async () => ({ result: 1 }));
        const task2 = step.do('task-2', async () => ({ result: 2 }));
        await Promise.all([task1, task2]);

        // ✅ GOOD: Direct calls in Promise.all
        await Promise.all([
            step.sleep('wait-1', '30 seconds'),
            step.do('task-3', async () => ({ result: 3 })),
            step.waitForEvent('approval', { timeout: '5 minutes', type: 'approval' }),
        ]);

        // ✅ GOOD: Using Promise.race
        const winner = await Promise.race([
            step.sleep('timeout', '1 minute'),
            step.waitForEvent('user-input', { type: 'input' }),
        ]);

        return { status: 'completed' };
    }
}
