import { describe, expect, test, vi } from "vitest";
import * as common from "@temporalio/common";

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
  ActivityOptions,
  SignalWithStartWorkflowExecutionRequest,
  WorkflowService,
  retryPolicyFromProto,
} from "./output.ts";

describe("workflow-service generated output", () => {
  test("exposes workflow service metadata", () => {
    expect(WorkflowService.name).toBe("WorkflowService");
    expect(
      WorkflowService.operations.signalWithStartWorkflowExecution.name,
    ).toBe(
      "SignalWithStartWorkflowExecution",
    );
  });

  test("round-trips activity options", () => {
    const retryPolicy = retryPolicyFromProto(
      common.compileRetryPolicy({ maximumAttempts: 3 }),
    );
    const activityOptions: ActivityOptions = {
      taskQueue: "demo-task-queue",
      retryPolicy,
      scheduleToCloseTimeout: "1 minute",
      priority: {
        priorityKey: 1,
        fairnessKey: "customer-123",
      },
    };

    const proto = ActivityOptions.toProto(activityOptions);
    expect(proto?.taskQueue?.name).toBe("demo-task-queue");
    expect(proto?.retryPolicy?.maximumAttempts).toBe(3);

    const roundTripped = ActivityOptions.fromProto(proto);
    expect(roundTripped?.taskQueue).toBe("demo-task-queue");
    expect(roundTripped?.retryPolicy.maximumAttempts).toBe(3);
    expect(roundTripped?.scheduleToCloseTimeout).toBe(60_000);
    expect(roundTripped?.priority?.priorityKey).toBe(1);
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
