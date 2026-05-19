import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vitest";
import type { temporal } from "@temporalio/proto";
import * as workflow from "@temporalio/workflow";
import * as nexus from "nexus-rpc";

import {
  SignalWithStartWorkflowExecutionRequest,
  WorkflowService,
  signalWithStartWorkflowExecution,
} from "../workflow-service/index.ts";
import {
  executeWorkflowWithNexus,
  withWorkflowEnvironment,
} from "./helpers.ts";

const workflowsPath = fileURLToPath(
  new URL("./workflows/workflow-service.ts", import.meta.url),
);

describe("workflow-service generated output", () => {
  test("exposes workflow service metadata", () => {
    expect(WorkflowService.name).toBe("WorkflowService");
    expect(
      WorkflowService.operations.signalWithStartWorkflowExecution.name,
    ).toBe("SignalWithStartWorkflowExecution");
  });

  test("serializes signal-with-start requests through a real Nexus client", async () => {
    await withWorkflowEnvironment(async (env) => {
      const calls: Array<[string, unknown]> = [];
      const handler = nexus.serviceHandler(WorkflowService, {
        async signalWithStartWorkflowExecution(_ctx, input) {
          calls.push(["SignalWithStartWorkflowExecution", input]);
          return {
            runId: "run-123",
            started: true,
          };
        },
      });

      const result = await executeWorkflowWithNexus<{
        runId: string | undefined;
        workflowId: string;
      }>(env, {
        endpoint: "temporal-system",
        nexusServices: [handler],
        workflowType: "workflowServiceCaller",
        workflowsPath,
      });

      expect(result).toEqual({
        runId: "run-123",
        workflowId: "workflow-id",
      });
      expect(calls).toHaveLength(1);

      const request = calls[0]?.[1] as
        | temporal.api.workflowservice.v1.ISignalWithStartWorkflowExecutionRequest
        | undefined;
      expect(request?.namespace).toBe("default");
      expect(request?.workflowType?.name).toBe("exampleWorkflow");
      expect(request?.workflowId).toBe("workflow-id");
      expect(request?.taskQueue?.name).toBe("demo-task-queue");
      expect(request?.signalName).toBe("wake-up");
      expect(request?.input?.payloads).toHaveLength(2);
      expect(request?.signalInput?.payloads).toHaveLength(2);
      expect(request?.workflowRunTimeout?.seconds).toMatchObject({ low: 300 });
    });
  });
});

if (false) {
  async function exampleWorkflow(
    attempts: number,
    note: string,
  ): Promise<void> {
    void attempts;
    void note;
  }

  const taskQueue = "demo-task-queue";
  const wakeUpSignal = workflow.defineSignal<[number, string]>("wake-up");

  const request: SignalWithStartWorkflowExecutionRequest<
    typeof exampleWorkflow,
    typeof wakeUpSignal
  > = {
    workflow: exampleWorkflow,
    input: [3, "nexus"],
    workflowId: "workflow-id",
    taskQueue,
    signal: wakeUpSignal,
    signalInput: [7, "hello"],
  };

  // @ts-expect-error sourced fields are not part of the generated request surface
  request.namespace;

  // @ts-expect-error missing workflow args for a callable workflow
  signalWithStartWorkflowExecution({
    workflow: exampleWorkflow,
    workflowId: "missing-workflow-input",
    taskQueue,
    signal: "wake-up",
  });

  // @ts-expect-error workflow args must match the workflow callable
  signalWithStartWorkflowExecution({
    workflow: exampleWorkflow,
    input: [3, 4],
    workflowId: "bad-workflow-input",
    taskQueue,
    signal: "wake-up",
  });

  // @ts-expect-error missing signal args for a signal definition
  signalWithStartWorkflowExecution({
    workflow: "ExampleWorkflow",
    workflowId: "missing-signal-input",
    taskQueue,
    signal: wakeUpSignal,
  });

  // @ts-expect-error signal args must match the signal definition
  signalWithStartWorkflowExecution({
    workflow: "ExampleWorkflow",
    workflowId: "bad-signal-input",
    taskQueue,
    signal: wakeUpSignal,
    signalInput: ["wrong", 7],
  });

  // @ts-expect-error request models are write-only
  SignalWithStartWorkflowExecutionRequest.fromProto({});
}
