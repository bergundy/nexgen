import * as common from "@temporalio/common";
import * as workflow from "@temporalio/workflow";

import {
  ActivityOptions,
  SignalWithStartWorkflowExecutionRequest,
  WorkflowService,
  WorkflowServiceClient,
  retryPolicyFromProto,
} from "./output.ts";

const retryPolicy = retryPolicyFromProto(
  common.compileRetryPolicy({ maximumAttempts: 3 }),
);

const taskQueue = "demo-task-queue";
const priority: common.Priority = {
  priorityKey: 1,
  fairnessKey: "customer-123",
};
const customerIdKey = common.defineSearchAttributeKey(
  "CustomerId",
  common.SearchAttributeType.KEYWORD,
);
const typedSearchAttributes = new common.TypedSearchAttributes([
  { key: customerIdKey, value: "customer-123" },
]);
const versioningOverride: common.VersioningOverride = "AUTO_UPGRADE";

const activityOptions: ActivityOptions = {
  taskQueue,
  retryPolicy,
  scheduleToCloseTimeout: "1 minute",
  priority,
};

async function exampleWorkflow(attempts: number, note: string): Promise<void> {
  void attempts;
  void note;
}

const wakeUpSignal = workflow.defineSignal<[number, string]>("wake-up");
const wakeUpManySignal = workflow.defineSignal<
  [number, number, number, number, number, number, number]
>("wake-up-many");

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
  workflowRunTimeout: "5 minutes",
  workflowIdReusePolicy: common.WorkflowIdReusePolicy.ALLOW_DUPLICATE,
  workflowIdConflictPolicy: common.WorkflowIdConflictPolicy.FAIL,
  retryPolicy,
  memo: {
    customerId: "customer-123",
    attempt: 3,
  },
  searchAttributes: typedSearchAttributes,
  versioningOverride,
  priority,
};

const activityProto = ActivityOptions.toProto(activityOptions);
const requestProto = SignalWithStartWorkflowExecutionRequest.toProto(request)!;
const stringNamedRequestProto = SignalWithStartWorkflowExecutionRequest.toProto(
  {
    workflow: "ExampleWorkflow",
    workflowId: "string-workflow-id",
    taskQueue,
    signal: "wake-up",
  },
);
const roundTrippedActivity = ActivityOptions.fromProto(activityProto);
const serviceName: string = WorkflowService.name;
const retryLimit = retryPolicy.maximumAttempts;
const nestedRetryLimit = roundTrippedActivity?.retryPolicy.maximumAttempts;
const roundTrippedTaskQueue: string | undefined =
  roundTrippedActivity?.taskQueue;
const roundTrippedTimeout: common.Duration | undefined =
  roundTrippedActivity?.scheduleToCloseTimeout;
const roundTrippedPriorityKey = roundTrippedActivity?.priority?.priorityKey;
const requestNamespace: string | null | undefined = requestProto.namespace;
const requestTaskQueueName: string | null | undefined =
  requestProto.taskQueue?.name;
const client = new WorkflowServiceClient();
const retryHandle = client.retryPolicyOperation(retryPolicy);
const signalHandle: Promise<workflow.ExternalWorkflowHandle> =
  client.signalWithStartWorkflowExecution({
    workflow: exampleWorkflow,
    input: [3, "nexus"],
    workflowId: "workflow-id",
    taskQueue,
    signal: wakeUpSignal,
    signalInput: [7, "hello"],
  });
const highAritySignalHandle: Promise<workflow.ExternalWorkflowHandle> =
  client.signalWithStartWorkflowExecution({
    workflow: "ExampleWorkflow",
    workflowId: "workflow-id-high-arity",
    taskQueue,
    signal: wakeUpManySignal,
    signalInput: [1, 2, 3, 4, 5, 6, 7],
  });

// @ts-expect-error sourced fields are not part of the generated request surface
request.namespace;

// @ts-expect-error missing workflow args for a callable workflow
client.signalWithStartWorkflowExecution({
  workflow: exampleWorkflow,
  workflowId: "missing-workflow-input",
  taskQueue,
  signal: "wake-up",
});

// @ts-expect-error workflow args must match the workflow callable
client.signalWithStartWorkflowExecution({
  workflow: exampleWorkflow,
  input: [3, 4],
  workflowId: "bad-workflow-input",
  taskQueue,
  signal: "wake-up",
});

// @ts-expect-error missing signal args for a signal definition
client.signalWithStartWorkflowExecution({
  workflow: "ExampleWorkflow",
  workflowId: "missing-signal-input",
  taskQueue,
  signal: wakeUpSignal,
});

// @ts-expect-error signal args must match the signal definition
client.signalWithStartWorkflowExecution({
  workflow: "ExampleWorkflow",
  workflowId: "bad-signal-input",
  taskQueue,
  signal: wakeUpSignal,
  signalInput: ["wrong", 7],
});

void serviceName;
void retryLimit;
void nestedRetryLimit;
void roundTrippedTaskQueue;
void roundTrippedTimeout;
void roundTrippedPriorityKey;
void requestNamespace;
void requestTaskQueueName;
void requestProto;
void stringNamedRequestProto;
void retryHandle;
void signalHandle;
void highAritySignalHandle;

// @ts-expect-error request models are write-only
SignalWithStartWorkflowExecutionRequest.fromProto({});
