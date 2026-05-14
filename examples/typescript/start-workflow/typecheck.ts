import type * as common from "@temporalio/common";
import {
  StartedWorkflow,
  StartWorkflowExecutionRequest,
  cancelWorkflow,
  startWorkflow,
} from "./output/index.ts";

async function exampleWorkflow(customerId: string): Promise<string> {
  return customerId;
}

const request: StartWorkflowExecutionRequest<typeof exampleWorkflow> = {
  workflow: exampleWorkflow,
  input: ["customer-123"],
  workflowId: "workflow-id",
  taskQueue: "demo-task-queue",
};

const handlePromise: Promise<StartedWorkflow> = startWorkflow(request);
const namedHandlePromise: Promise<StartedWorkflow> = startWorkflow({
  workflow: "ExampleWorkflow",
  workflowId: "workflow-id-named",
  taskQueue: "demo-task-queue",
});
void cancelWorkflow({
  workflowExecution: {
    workflowId: "workflow-id",
  },
});

async function useHandle(): Promise<void> {
  const handle = await handlePromise;
  await handle.cancel();
  const restartedHandlePromise: Promise<StartedWorkflow> = handle.restartWorkflow(
    exampleWorkflow,
    "demo-task-queue",
  );
  void restartedHandlePromise;
  const resultPromise: Promise<common.Payload[]> = handle.getResult();
  void resultPromise;
}

void namedHandlePromise;
void useHandle;

// @ts-expect-error missing workflow args for a callable workflow
startWorkflow({
  workflow: exampleWorkflow,
  workflowId: "missing-input",
  taskQueue: "demo-task-queue",
});

// @ts-expect-error workflow args must match the workflow callable
startWorkflow({
  workflow: exampleWorkflow,
  input: [7],
  workflowId: "bad-input",
  taskQueue: "demo-task-queue",
});
