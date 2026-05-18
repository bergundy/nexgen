import { describe, expect, test, vi } from "vitest";

vi.mock("@temporalio/workflow", async () => {
  const actual = await vi.importActual<typeof import("@temporalio/workflow")>(
    "@temporalio/workflow",
  );
  return {
    ...actual,
    workflowInfo: () =>
      ({
        namespace: "workflow-namespace",
      }) as ReturnType<typeof actual.workflowInfo>,
  };
});

import * as workflow from "@temporalio/workflow";

import {
  SignalWithStartWorkflowExecutionRequest,
  WorkflowService,
  signalWithStartWorkflowExecution,
} from "../workflow-service/index.ts";

describe("workflow-service generated output", () => {
  test("exposes workflow service metadata", () => {
    expect(WorkflowService.name).toBe("WorkflowService");
    expect(
      WorkflowService.operations.signalWithStartWorkflowExecution.name,
    ).toBe("SignalWithStartWorkflowExecution");
  });

  test("serializes signal-with-start requests", () => {
    async function exampleWorkflow(
      attempts: number,
      note: string,
    ): Promise<void> {
      void attempts;
      void note;
    }

    const wakeUpSignal = workflow.defineSignal<[number, string]>("wake-up");
    const proto = SignalWithStartWorkflowExecutionRequest.toProto({
      workflow: exampleWorkflow,
      input: [3, "nexus"],
      workflowId: "workflow-id",
      taskQueue: "demo-task-queue",
      signal: wakeUpSignal,
      signalInput: [7, "hello"],
      workflowRunTimeout: "5 minutes",
    });

    expect(proto?.workflowType?.name).toBe("exampleWorkflow");
    expect(proto?.workflowId).toBe("workflow-id");
    expect(proto?.taskQueue?.name).toBe("demo-task-queue");
    expect(proto?.signalName).toBe("wake-up");
    expect(proto?.input?.payloads).toHaveLength(2);
    expect(proto?.signalInput?.payloads).toHaveLength(2);
    expect(proto?.namespace).toBe("workflow-namespace");
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
