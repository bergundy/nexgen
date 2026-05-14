import * as common from "@temporalio/common";
import * as workflow from "@temporalio/workflow";
import type { temporal } from "@temporalio/proto";

import {
  ActivityOptions,
  TypeRoundtripService,
  activityOptionsOperation,
  retryPolicyFromProto,
  retryPolicyOperation,
} from "./output/index.ts";

const retryPolicy = retryPolicyFromProto(
  common.compileRetryPolicy({ maximumAttempts: 3 }),
);

const taskQueue = "demo-task-queue";
const priority: common.Priority = {
  priorityKey: 1,
  fairnessKey: "customer-123",
};

const activityOptions: ActivityOptions = {
  taskQueue,
  retryPolicy,
  scheduleToCloseTimeout: "1 minute",
  priority,
};

const activityProto = ActivityOptions.toProto(activityOptions);
const roundTrippedActivity = ActivityOptions.fromProto(activityProto);
const serviceName: string = TypeRoundtripService.name;
const retryLimit = retryPolicy.maximumAttempts;
const nestedRetryLimit = roundTrippedActivity?.retryPolicy.maximumAttempts;
const roundTrippedTaskQueue: string | undefined =
  roundTrippedActivity?.taskQueue;
const roundTrippedTimeout: common.Duration | undefined =
  roundTrippedActivity?.scheduleToCloseTimeout;
const roundTrippedPriorityKey = roundTrippedActivity?.priority?.priorityKey;
const retryHandle = retryPolicyOperation(retryPolicy);

void serviceName;
void retryLimit;
void nestedRetryLimit;
void roundTrippedTaskQueue;
void roundTrippedTimeout;
void roundTrippedPriorityKey;
void activityOptionsOperation(activityOptions);
void retryHandle;

const typedRetryHandle: Promise<
  workflow.NexusOperationHandle<temporal.api.common.v1.IRetryPolicy>
> = retryPolicyOperation(retryPolicy);
const typedActivityHandle: Promise<
  workflow.NexusOperationHandle<temporal.api.activity.v1.IActivityOptions>
> = activityOptionsOperation(activityOptions);

void typedRetryHandle;
void typedActivityHandle;
