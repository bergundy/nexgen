import * as common from '@temporalio/common';

import {
  ActivityOptions,
  SignalWithStartWorkflowExecutionRequest,
  TaskQueueKind,
  TaskQueue,
  WorkflowService,
  WorkflowServiceClient,
  retryPolicyFromProto,
} from './output.ts';

const retryPolicy = retryPolicyFromProto(
  common.compileRetryPolicy({ maximumAttempts: 3 }),
);

const taskQueue: TaskQueue = {
  name: 'demo-task-queue',
  kind: TaskQueueKind.TASK_QUEUE_KIND_NORMAL,
};

const activityOptions: ActivityOptions = {
  taskQueue,
  retryPolicy,
};

const request: SignalWithStartWorkflowExecutionRequest = {
  workflowType: { name: 'ExampleWorkflow' },
  workflowId: 'workflow-id',
  taskQueue,
  signalName: 'wake-up',
};

const activityProto = ActivityOptions.toProto(activityOptions);
const requestProto = SignalWithStartWorkflowExecutionRequest.toProto(request);
const roundTrippedActivity = ActivityOptions.fromProto(activityProto);
const serviceName: string = WorkflowService.name;
const retryLimit = retryPolicy.maximumAttempts;
const nestedRetryLimit = roundTrippedActivity?.retryPolicy.maximumAttempts;
const client = new WorkflowServiceClient();
const retryHandle = client.retryPolicyOperation(retryPolicy);
const signalHandle = client.signalWithStartWorkflowExecution(request);

void serviceName;
void retryLimit;
void nestedRetryLimit;
void requestProto;
void retryHandle;
void signalHandle;

// @ts-expect-error request models are write-only
SignalWithStartWorkflowExecutionRequest.fromProto({});
