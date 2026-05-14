import type * as common from "@temporalio/common";
import { StartedWorkflow, StartWorkflowExecutionRequest, WorkflowServiceClient } from "./output.ts";

async function exampleWorkflow(customerId: string): Promise<string> {
  return customerId;
}

const request: StartWorkflowExecutionRequest<typeof exampleWorkflow> = {
  workflow: exampleWorkflow,
  input: ["customer-123"],
  workflowId: "workflow-id",
  taskQueue: "demo-task-queue",
};

const client = new WorkflowServiceClient();
const handlePromise: Promise<StartedWorkflow> = client.startWorkflow(
  request,
);
const namedHandlePromise: Promise<StartedWorkflow> = client.startWorkflow(
  {
    workflow: "ExampleWorkflow",
    workflowId: "workflow-id-named",
    taskQueue: "demo-task-queue",
  },
);
void client.cancelWorkflow({
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
client.startWorkflow({
  workflow: exampleWorkflow,
  workflowId: "missing-input",
  taskQueue: "demo-task-queue",
});

// @ts-expect-error workflow args must match the workflow callable
client.startWorkflow({
  workflow: exampleWorkflow,
  input: [7],
  workflowId: "bad-input",
  taskQueue: "demo-task-queue",
});
