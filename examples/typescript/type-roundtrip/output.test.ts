import { describe, expect, test } from "vitest";
import * as common from "@temporalio/common";

import {
  ActivityOptions,
  TypeRoundtripService,
  retryPolicyFromProto,
} from "./output/index.ts";

describe("type-roundtrip generated output", () => {
  test("exposes type roundtrip service metadata", () => {
    expect(TypeRoundtripService.name).toBe("TypeRoundtripService");
    expect(TypeRoundtripService.operations.retryPolicyOperation.name).toBe(
      "RetryPolicyOperation",
    );
    expect(TypeRoundtripService.operations.activityOptionsOperation.name).toBe(
      "ActivityOptionsOperation",
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
});
