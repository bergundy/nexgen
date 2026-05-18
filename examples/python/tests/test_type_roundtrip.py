from __future__ import annotations

from collections.abc import Generator
import datetime
from pathlib import Path

from nexusrpc import Operation
import pytest
import temporalio.api.activity.v1 as activity_v1
import temporalio.api.common.v1
import temporalio.common
import temporalio.workflow

import type_roundtrip as output
import type_roundtrip.models as output_models
import type_roundtrip._support as output_support

OUTPUT_PATH = Path(__file__).resolve().parent.parent / "type_roundtrip"
TASK_QUEUE = "demo-task-queue"

RETRY_POLICY_OPERATION = output.__nexus_operation_registry__[
    ("TypeRoundtripService", "RetryPolicyOperation")
]
ACTIVITY_OPTIONS_OPERATION = output.__nexus_operation_registry__[
    ("TypeRoundtripService", "ActivityOptionsOperation")
]

ResponseProto = activity_v1.ActivityOptions | temporalio.api.common.v1.RetryPolicy


class FakeOperationHandle:
    def __init__(self, response: ResponseProto) -> None:
        self._response: ResponseProto = response

    def cancel(self) -> bool:
        return False

    @property
    def operation_token(self) -> str | None:
        return None

    def __await__(self) -> Generator[object, None, ResponseProto]:
        async def wait_for_result() -> ResponseProto:
            return self._response

        return wait_for_result().__await__()


class FakeNexusClient:
    def __init__(self) -> None:
        self.calls: list[tuple[str, object]] = []

    async def start_operation(
        self,
        operation: str,
        input: object,
        *,
        output_type: type[object] | None = None,
    ) -> FakeOperationHandle:
        self.calls.append((operation, input))

        if operation == "RetryPolicyOperation":
            assert output_type is temporalio.api.common.v1.RetryPolicy
            assert isinstance(input, temporalio.api.common.v1.RetryPolicy)
            response = temporalio.api.common.v1.RetryPolicy()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        if operation == "ActivityOptionsOperation":
            assert output_type is activity_v1.ActivityOptions
            assert isinstance(input, activity_v1.ActivityOptions)
            assert input.HasField("retry_policy")
            assert input.task_queue.name == TASK_QUEUE
            assert input.schedule_to_close_timeout.seconds == 7
            assert input.priority.priority_key == 4
            response = activity_v1.ActivityOptions()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        raise AssertionError(f"unexpected operation: {operation}")


@pytest.fixture
def fake_client() -> FakeNexusClient:
    fake_client = FakeNexusClient()

    def fake_create_nexus_client(*, service: str, endpoint: str) -> FakeNexusClient:
        assert service == "TypeRoundtripService"
        assert endpoint == "__temporal_system"
        return fake_client

    setattr(temporalio.workflow, "create_nexus_client", fake_create_nexus_client)
    return fake_client


@pytest.fixture
def retry_policy() -> temporalio.common.RetryPolicy:
    return temporalio.common.RetryPolicy(maximum_attempts=3)


@pytest.fixture
def priority() -> temporalio.common.Priority:
    return temporalio.common.Priority(
        priority_key=4,
        fairness_key="tenant-a",
        fairness_weight=2.5,
    )


def test_generated_metadata() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated package at {OUTPUT_PATH}"
    assert isinstance(RETRY_POLICY_OPERATION, Operation)
    assert isinstance(ACTIVITY_OPTIONS_OPERATION, Operation)
    assert (
        output.__nexus_operation_registry__[
            ("TypeRoundtripService", "RetryPolicyOperation")
        ]
        is RETRY_POLICY_OPERATION
    )
    assert (
        output.__nexus_operation_registry__[
            ("TypeRoundtripService", "ActivityOptionsOperation")
        ]
        is ACTIVITY_OPTIONS_OPERATION
    )
    assert not hasattr(output, "TypeRoundtripService")
    assert not hasattr(output, "ActivityOptions")


def test_activity_options_round_trip(
    retry_policy: temporalio.common.RetryPolicy,
    priority: temporalio.common.Priority,
) -> None:
    activity_options = output_models.ActivityOptions(
        retry_policy=retry_policy,
        task_queue=TASK_QUEUE,
        schedule_to_close_timeout=datetime.timedelta(seconds=7),
        priority=priority,
    )
    activity_proto = activity_options.to_proto()
    assert activity_proto.HasField("retry_policy")
    assert activity_proto.task_queue.name == TASK_QUEUE
    assert activity_proto.schedule_to_close_timeout.seconds == 7
    assert activity_proto.priority.priority_key == 4
    round_tripped_activity = output_models.ActivityOptions.from_proto(activity_proto)
    assert isinstance(
        round_tripped_activity.retry_policy, temporalio.common.RetryPolicy
    )
    assert round_tripped_activity.retry_policy.maximum_attempts == 3
    assert round_tripped_activity.task_queue == TASK_QUEUE
    assert round_tripped_activity.schedule_to_close_timeout == datetime.timedelta(
        seconds=7
    )
    assert round_tripped_activity.priority == priority


@pytest.mark.asyncio
async def test_retry_policy_operation(
    fake_client: FakeNexusClient,
    retry_policy: temporalio.common.RetryPolicy,
) -> None:
    retry_handle = await output.retry_policy_operation(retry_policy)
    retry_round_trip = output_support.retry_policy_from_proto(await retry_handle)
    assert retry_round_trip.maximum_attempts == 3
    assert len(fake_client.calls) == 1


@pytest.mark.asyncio
async def test_activity_options_operation(
    fake_client: FakeNexusClient,
    retry_policy: temporalio.common.RetryPolicy,
    priority: temporalio.common.Priority,
) -> None:
    activity_handle = await output.activity_options_operation(
        task_queue=TASK_QUEUE,
        schedule_to_close_timeout=datetime.timedelta(seconds=7),
        retry_policy=retry_policy,
        priority=priority,
    )
    activity_response = output_models.ActivityOptions.from_proto(await activity_handle)
    assert isinstance(activity_response.retry_policy, temporalio.common.RetryPolicy)
    assert activity_response.retry_policy.maximum_attempts == 3
    assert activity_response.task_queue == TASK_QUEUE
    assert activity_response.schedule_to_close_timeout == datetime.timedelta(seconds=7)
    assert activity_response.priority == priority
    assert len(fake_client.calls) == 1
