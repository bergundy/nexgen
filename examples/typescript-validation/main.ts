import * as common from '@temporalio/common';
import * as workflow from '@temporalio/workflow';

import {
  ActivityOptionsModel,
  DurationModel,
  PriorityModel,
  RetryPolicyModel,
  SignalWithStartWorkflowExecutionRequestModel,
  TaskQueueKind,
  TaskQueueModel,
  WorkflowService,
  WorkflowServiceClient,
  type WorkflowServiceRoundTripActivityOptionsOptions,
  type WorkflowServiceSignalWithStartWorkflowOptions,
} from './output.ts';

const retryPolicy = RetryPolicyModel.fromProto(
  common.compileRetryPolicy({ maximumAttempts: 3 }),
);

const duration = DurationModel.fromProto(common.msToTs('5s'));

const taskQueue: TaskQueueModel = {
  name: 'demo-task-queue',
  kind: TaskQueueKind.TASK_QUEUE_KIND_NORMAL,
};

const activityOptions: ActivityOptionsModel = {
  taskQueue,
  scheduleToCloseTimeout: duration,
  retryPolicy,
  priority: PriorityModel.fromProto(common.compilePriority({})) ?? {},
};

const request: SignalWithStartWorkflowExecutionRequestModel = {
  workflowId: 'workflow-id',
  signalName: 'signal-name',
  taskQueue,
};

const signalOptions: WorkflowServiceSignalWithStartWorkflowOptions = {
  signal: 'signal-name',
  workflowId: 'workflow-id',
  priority: {},
};

const roundTripActivityOptions: WorkflowServiceRoundTripActivityOptionsOptions = {
  taskQueue: 'demo-task-queue',
  retryPolicy: { maximumAttempts: 1 },
};

const serviceName: string = WorkflowService.name;
const activityRetryPolicy = activityOptions.retryPolicy;
const signalWorkflowId = request.workflowId;
const client = new WorkflowServiceClient();
const lowLevelResponse = RetryPolicyModel.fromProto(common.compileRetryPolicy({ maximumAttempts: 1 })) ?? {};
const lowLevelHandle = client.retryPolicyOperation(lowLevelResponse);
const roundTripResult: Promise<common.RetryPolicy | undefined> =
  client.roundTripRetryPolicy(undefined);
const signalWithStartResult: Promise<workflow.ExternalWorkflowHandle> =
  client.signalWithStartWorkflow('exampleWorkflow', signalOptions);

void serviceName;
void signalOptions;
void roundTripActivityOptions;
void signalWorkflowId;
void activityRetryPolicy;
void lowLevelHandle;
void roundTripResult;
void signalWithStartResult;
