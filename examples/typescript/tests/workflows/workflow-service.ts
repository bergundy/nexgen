import * as workflow from "@temporalio/workflow";
import { signalWithStartWorkflowExecution } from "../../workflow-service/index.ts";

export async function exampleWorkflow(
  attempts: number,
  note: string,
): Promise<void> {
  void attempts;
  void note;
}

const wakeUpSignal = workflow.defineSignal<[number, string]>("wake-up");

export async function workflowServiceCaller(): Promise<{
  runId: string | undefined;
  workflowId: string;
}> {
  const handle = await signalWithStartWorkflowExecution({
    workflow: exampleWorkflow,
    input: [3, "nexus"],
    workflowId: "workflow-id",
    taskQueue: "demo-task-queue",
    signal: wakeUpSignal,
    signalInput: [7, "hello"],
    workflowRunTimeout: "5 minutes",
  });
  return {
    runId: handle.runId,
    workflowId: handle.workflowId,
  };
}
