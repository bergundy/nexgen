import { describe, expect, test, vi } from "vitest";

const runtime = vi.hoisted(() => {
  const startOperation = vi.fn();
  const createNexusServiceClient = vi.fn(() => ({
    startOperation,
  }));
  return {
    startOperation,
    createNexusServiceClient,
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
  };
});

import {
  RequestCancelWorkflowExecutionRequest,
  StartedWorkflow,
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
    expect(WorkflowService.operations.restartWorkflow.name).toBe("RestartWorkflow");
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

    runtime.startOperation
      .mockResolvedValueOnce({
        result: async () => ({
          runId: "run-123",
          started: true,
        }),
      })
      .mockResolvedValueOnce({
        result: async () => ({
          runId: "run-456",
          started: true,
        }),
      })
      .mockResolvedValueOnce({
        result: async () => ({}),
      });

    const client = new WorkflowServiceClient();
    const handle = await client.startWorkflow({
      workflow: exampleWorkflow,
      input: ["customer-123"],
      workflowId: "workflow-id",
      taskQueue: "demo-task-queue",
    });

    expect(handle).toBeInstanceOf(StartedWorkflow);
    expect(handle.namespace).toBe("workflow-namespace");
    expect(handle.workflowId).toBe("workflow-id");
    expect(handle.runId).toBe("run-123");
    expect(runtime.createNexusServiceClient).toHaveBeenCalledWith({
      service: WorkflowService,
      endpoint: "__temporal_system",
    });

    const restartedHandle = await handle.restartWorkflow(
      exampleWorkflow,
      "demo-task-queue",
    );

    expect(restartedHandle).toBeInstanceOf(StartedWorkflow);
    expect(restartedHandle.namespace).toBe("workflow-namespace");
    expect(restartedHandle.workflowId).toBe("workflow-id");
    expect(restartedHandle.runId).toBe("run-456");
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      2,
      WorkflowService.operations.restartWorkflow,
      {
        namespace: "workflow-namespace",
        workflowId: "workflow-id",
        workflowType: {
          name: "exampleWorkflow",
        },
        taskQueue: {
          name: "demo-task-queue",
        },
      },
    );

    await handle.cancel();
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      3,
      WorkflowService.operations.cancelWorkflow,
      {
        namespace: "workflow-namespace",
        workflowExecution: {
          workflowId: "workflow-id",
          runId: "run-123",
        },
        reason: undefined,
      },
    );

    await expect(handle.getResult()).rejects.toThrow(
      "started-workflow.getResult is not yet implemented",
    );
  });
});
