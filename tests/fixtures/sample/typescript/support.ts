import { mapToPayloads, toPayloads } from '@temporalio/common/lib/converter/payload-converter.js';
import { encodeUnifiedSearchAttributes } from '@temporalio/common/lib/converter/payload-search-attributes.js';
import { userMetadataToPayload } from '@temporalio/common/lib/user-metadata.js';
import { getActivator } from '@temporalio/workflow/lib/global-attributes.js';

function signalName(signal: string | workflow.SignalDefinition<any[]>): string {
  return typeof signal === 'string' ? signal : signal.name;
}

export function buildSignalWithStartWorkflowRequest({
  workflowTypeOrFunc,
  signal,
  workflowId,
  signalArgs,
  args,
  taskQueue,
  workflowExecutionTimeout,
  workflowRunTimeout,
  workflowTaskTimeout,
  workflowIdReusePolicy,
  workflowIdConflictPolicy,
  retry,
  cronSchedule,
  memo,
  typedSearchAttributes,
  staticSummary,
  staticDetails,
  startDelay,
  requestId,
  priority,
  versioningOverride,
}: {
  workflowTypeOrFunc: string | workflow.Workflow;
  signal: string | workflow.SignalDefinition<any[]>;
  workflowId: string;
  signalArgs: readonly unknown[];
  args: readonly unknown[];
  taskQueue: string | undefined;
  workflowExecutionTimeout: common.Duration | undefined;
  workflowRunTimeout: common.Duration | undefined;
  workflowTaskTimeout: common.Duration | undefined;
  workflowIdReusePolicy: workflow.WorkflowIdReusePolicy;
  workflowIdConflictPolicy: workflow.WorkflowIdConflictPolicy | undefined;
  retry: common.RetryPolicy | undefined;
  cronSchedule: string;
  memo: Record<string, unknown> | undefined;
  typedSearchAttributes: common.TypedSearchAttributes | common.SearchAttributePair[] | undefined;
  staticSummary: string | undefined;
  staticDetails: string | undefined;
  startDelay: common.Duration | undefined;
  requestId: string | undefined;
  priority: common.Priority;
  versioningOverride: common.VersioningOverride | undefined;
}): SignalWithStartWorkflowExecutionRequestModel {
  const activator = getActivator();
  const info = workflow.workflowInfo();
  const request = {
    namespace: info.namespace,
    workflowId,
    workflowType: {
      name: common.extractWorkflowType(workflowTypeOrFunc),
    },
    input: args.length
      ? {
          payloads: toPayloads(activator.payloadConverter, ...args),
        }
      : undefined,
    signalName: signalName(signal),
    signalInput: signalArgs.length
      ? {
          payloads: toPayloads(activator.payloadConverter, ...signalArgs),
        }
      : undefined,
    taskQueue: {
      kind: TaskQueueKind.TASK_QUEUE_KIND_NORMAL,
      name: taskQueue ?? info.taskQueue,
    },
    workflowExecutionTimeout:
      workflowExecutionTimeout == null ? undefined : common.msToTs(workflowExecutionTimeout),
    workflowRunTimeout:
      workflowRunTimeout == null ? undefined : common.msToTs(workflowRunTimeout),
    workflowTaskTimeout:
      workflowTaskTimeout == null ? undefined : common.msToTs(workflowTaskTimeout),
    requestId,
    workflowIdReusePolicy:
      common.encodeWorkflowIdReusePolicy(workflowIdReusePolicy) as WorkflowIdReusePolicy,
    workflowIdConflictPolicy:
      workflowIdConflictPolicy == null
        ? undefined
        : (common.encodeWorkflowIdConflictPolicy(workflowIdConflictPolicy) as WorkflowIdConflictPolicy),
    retryPolicy: retry == null ? undefined : common.compileRetryPolicy(retry),
    cronSchedule,
    memo:
      memo == null
        ? undefined
        : {
            fields: mapToPayloads(activator.payloadConverter, memo),
          },
    searchAttributes:
      typedSearchAttributes == null
        ? undefined
        : {
            indexedFields: encodeUnifiedSearchAttributes(undefined, typedSearchAttributes),
          },
    userMetadata: userMetadataToPayload(
      activator.payloadConverter,
      staticSummary,
      staticDetails,
    ),
    priority: common.compilePriority(priority),
    workflowStartDelay: startDelay == null ? undefined : common.msToTs(startDelay),
    versioningOverride: versioningOverrideToProto(versioningOverride),
  };
  return SignalWithStartWorkflowExecutionRequestModel.fromProto(request) ?? {};
}

export function sdkRetryPolicyToModel(
  retryPolicy: common.RetryPolicy | undefined,
): RetryPolicyModel | undefined {
  if (retryPolicy == null) {
    return undefined;
  }
  return RetryPolicyModel.fromProto(common.compileRetryPolicy(retryPolicy));
}

export function retryPolicyModelToSdk(
  retryPolicy: RetryPolicyModel | undefined,
): common.RetryPolicy | undefined {
  return common.decompileRetryPolicy(RetryPolicyModel.toProto(retryPolicy));
}

export function sdkTaskQueueToModel(
  taskQueue: string | undefined,
): TaskQueueModel | undefined {
  if (taskQueue == null) {
    return undefined;
  }
  return { name: taskQueue };
}

export function taskQueueModelToSdk(
  taskQueue: TaskQueueModel | undefined,
): string | undefined {
  return taskQueue?.name;
}

export function durationToModel(
  duration: common.Duration | undefined,
): DurationModel | undefined {
  if (duration == null) {
    return undefined;
  }
  return DurationModel.fromProto(common.msToTs(duration));
}

export function durationModelToDuration(
  duration: DurationModel | undefined,
): common.Duration | undefined {
  return common.optionalTsToMs(DurationModel.toProto(duration));
}

export function sdkPriorityToModel(priority: common.Priority): PriorityModel {
  return PriorityModel.fromProto(common.compilePriority(priority)) ?? {};
}

export function priorityModelToSdk(
  priority: PriorityModel | undefined,
): common.Priority {
  return common.decodePriority(PriorityModel.toProto(priority));
}

function versioningOverrideToProto(
  versioningOverride: common.VersioningOverride | undefined,
): VersioningOverrideModel | undefined {
  if (versioningOverride == null) {
    return undefined;
  }
  if (versioningOverride === 'AUTO_UPGRADE') {
    return {
      behavior: VersioningBehavior.VERSIONING_BEHAVIOR_AUTO_UPGRADE,
      autoUpgrade: true,
    };
  }
  return {
    behavior: VersioningBehavior.VERSIONING_BEHAVIOR_PINNED,
    pinnedVersion: common.toCanonicalString(versioningOverride.pinnedTo),
    pinned: {
      version: {
        buildId: versioningOverride.pinnedTo.buildId,
        deploymentName: versioningOverride.pinnedTo.deploymentName,
      },
      behavior:
        PinnedOverrideBehavior.PINNED_OVERRIDE_BEHAVIOR_PINNED,
    },
  };
}

export function signalWithStartWorkflowResponseToExternalWorkflowHandle({
  request,
  response,
}: {
  request: SignalWithStartWorkflowExecutionRequestModel;
  response: SignalWithStartWorkflowExecutionResponseModel;
}): workflow.ExternalWorkflowHandle {
  if (request.workflowId == null) {
    throw new TypeError('workflowId must be set to build an ExternalWorkflowHandle');
  }
  return workflow.getExternalWorkflowHandle(request.workflowId, response.runId);
}

export function activityOptionsModelToOptions({
  request,
  response,
}: {
  request: ActivityOptionsModel;
  response: ActivityOptionsModel;
}): workflow.ActivityOptions {
  const taskQueue = response.taskQueue ?? request.taskQueue;
  const scheduleToCloseTimeout =
    response.scheduleToCloseTimeout ?? request.scheduleToCloseTimeout;
  const scheduleToStartTimeout =
    response.scheduleToStartTimeout ?? request.scheduleToStartTimeout;
  const startToCloseTimeout =
    response.startToCloseTimeout ?? request.startToCloseTimeout;
  const heartbeatTimeout =
    response.heartbeatTimeout ?? request.heartbeatTimeout;
  const retryPolicy = response.retryPolicy ?? request.retryPolicy;
  const priority = response.priority ?? request.priority;
  return {
    taskQueue: taskQueueModelToSdk(taskQueue),
    scheduleToCloseTimeout: durationModelToDuration(scheduleToCloseTimeout),
    scheduleToStartTimeout: durationModelToDuration(scheduleToStartTimeout),
    startToCloseTimeout: durationModelToDuration(startToCloseTimeout),
    heartbeatTimeout: durationModelToDuration(heartbeatTimeout),
    retry: retryPolicyModelToSdk(retryPolicy),
    priority: priorityModelToSdk(priority),
  };
}
