import { describe, expect, test, vi } from "vitest";

const runtime = vi.hoisted(() => {
  const cancel = vi.fn(async () => undefined);
  const signal = vi.fn(async () => undefined);
  const startOperation = vi.fn();
  const createNexusServiceClient = vi.fn(() => ({
    startOperation,
  }));
  const getExternalWorkflowHandle = vi.fn(
    (workflowId: string, runId?: string) => ({
      workflowId,
      runId,
      cancel,
      signal,
    }),
  );
  return {
    cancel,
    signal,
    startOperation,
    createNexusServiceClient,
    getExternalWorkflowHandle,
  };
});

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
    createNexusServiceClient: runtime.createNexusServiceClient,
    getExternalWorkflowHandle: runtime.getExternalWorkflowHandle,
  };
});

import {
  RequestCancelWorkflowExecutionRequest,
  StartedWorkflowHandle,
  StartWorkflowExecutionRequest,
  WorkflowService,
  WorkflowServiceClient,
} from "./output.ts";

describe("start-workflow generated output", () => {
  test("serializes start-workflow requests", () => {
    async function exampleWorkflow(customerId: string): Promise<string> {
      return customerId;
    }

    const proto = StartWorkflowExecutionRequest.toProto({
      workflow: exampleWorkflow,
      input: ["customer-123"],
      workflowId: "workflow-id",
      taskQueue: "demo-task-queue",
    });

    expect(proto?.workflowType?.name).toBe("exampleWorkflow");
    expect(proto?.workflowId).toBe("workflow-id");
    expect(proto?.taskQueue?.name).toBe("demo-task-queue");
    expect(proto?.input?.payloads).toHaveLength(1);
    expect(proto?.namespace).toBe("workflow-namespace");
  });

  test("exposes workflow service metadata", () => {
    expect(WorkflowService.name).toBe("WorkflowService");
    expect(WorkflowService.operations.startWorkflow.name).toBe("StartWorkflow");
    expect(WorkflowService.operations.cancelWorkflow.name).toBe("CancelWorkflow");
  });

  test("serializes cancel-workflow requests", () => {
    const proto = RequestCancelWorkflowExecutionRequest.toProto({
      workflowExecution: {
        workflowId: "workflow-id",
      },
      reason: "user requested cancellation",
    });

    expect(proto?.namespace).toBe("workflow-namespace");
    expect(proto?.workflowExecution?.workflowId).toBe("workflow-id");
    expect(proto?.workflowExecution?.runId).toBeUndefined();
    expect(proto?.reason).toBe("user requested cancellation");
  });

  test("returns a started workflow wrapper handle", async () => {
    async function exampleWorkflow(customerId: string): Promise<string> {
      return customerId;
    }

    runtime.startOperation.mockResolvedValue({
      result: async () => ({
        runId: "run-123",
        started: true,
      }),
    });

    const client = new WorkflowServiceClient();
    const handle = await client.startWorkflow({
      workflow: exampleWorkflow,
      input: ["customer-123"],
      workflowId: "workflow-id",
      taskQueue: "demo-task-queue",
    });

    expect(handle).toBeInstanceOf(StartedWorkflowHandle);
    expect(handle.workflowId).toBe("workflow-id");
    expect(handle.runId).toBe("run-123");
    expect(runtime.createNexusServiceClient).toHaveBeenCalledWith({
      service: WorkflowService,
      endpoint: "__temporal_system",
    });
    expect(runtime.getExternalWorkflowHandle).toHaveBeenCalledWith(
      "workflow-id",
      "run-123",
    );

    await handle.cancel();
    expect(runtime.cancel).toHaveBeenCalled();

    await expect(handle.getResult()).rejects.toThrow(
      "result retrieval is not yet implemented",
    );
  });
});
