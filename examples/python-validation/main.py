from __future__ import annotations

import asyncio
from collections.abc import Generator, Sequence
from datetime import timedelta
from pathlib import Path
from typing import final, override

from nexusrpc import Operation
import temporalio.api.activity.v1 as activity_v1
import temporalio.api.common.v1 as common_v1
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.common as temporal_common
import temporalio.converter
from temporalio import workflow

import output

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT / "output.py"


ResponseProto = (
    activity_v1.ActivityOptions
    |
    common_v1.RetryPolicy
    | workflowservice_v1.SignalWithStartWorkflowExecutionResponse
)


@workflow.defn
class ExampleWorkflow:
    @workflow.run
    async def run(self) -> None:
        raise RuntimeError("validation workflow should never execute")

    @workflow.signal
    def wake_up(self) -> None:
        pass


@final
class FakeWorkflowInfo:
    namespace: str
    task_queue: str

    def __init__(self, namespace: str, task_queue: str) -> None:
        self.namespace = namespace
        self.task_queue = task_queue


@final
class FakePayloadConverter:
    @staticmethod
    def _encode(value: object) -> bytes:
        return f"{type(value).__name__}:{value!r}".encode()

    def to_payload(self, value: object) -> common_v1.Payload:
        return common_v1.Payload(data=self._encode(value))

    def to_payloads(self, values: Sequence[object]) -> list[common_v1.Payload]:
        return [self.to_payload(value) for value in values]


@final
class FakeOperationHandle(workflow.NexusOperationHandle[object]):
    _response: ResponseProto

    def __init__(
        self,
        response: ResponseProto,
    ) -> None:
        self._response = response

    @override
    def cancel(self) -> bool:
        return False

    @override
    def __await__(
        self,
    ) -> Generator[object, None, ResponseProto]:
        async def wait_for_result() -> ResponseProto:
            return self._response

        return wait_for_result().__await__()

    @property
    @override
    def operation_token(self) -> str | None:
        return None


@final
class FakeExternalWorkflowHandle(workflow.ExternalWorkflowHandle[object]):
    _id: str
    _run_id: str | None

    def __init__(self, workflow_id: str, run_id: str | None) -> None:
        self._id = workflow_id
        self._run_id = run_id

    @property
    @override
    def id(self) -> str:
        return self._id

    @property
    @override
    def run_id(self) -> str | None:
        return self._run_id


@final
class FakeNexusClient:
    calls: list[tuple[Operation[object, object], object]]

    def __init__(self) -> None:
        self.calls = []

    async def start_operation(
        self,
        operation: Operation[object, object],
        input: object,
    ) -> FakeOperationHandle:
        self.calls.append((operation, input))
        if operation is output.WorkflowService.signal_with_start_workflow_execution:
            assert isinstance(
                input,
                workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
            )
            assert input.namespace == "default"
            assert input.workflow_id == "workflow-123"
            assert input.signal_name == "wake_up"
            assert input.workflow_type.name == "ExampleWorkflow"
            assert input.task_queue.name in {"default", "current-task-queue"}

            response = workflowservice_v1.SignalWithStartWorkflowExecutionResponse()
            response.run_id = "run-123"
            response.started = True
            return FakeOperationHandle(response)

        if operation is output.WorkflowService.retry_policy_operation:
            assert isinstance(input, common_v1.RetryPolicy)
            response = common_v1.RetryPolicy()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        if operation is output.WorkflowService.activity_options_operation:
            assert isinstance(input, activity_v1.ActivityOptions)
            response = activity_v1.ActivityOptions()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        raise AssertionError(f"unexpected operation: {operation.name}")


async def main() -> None:
    created_clients: list[tuple[str, type[object]]] = []
    created_handles: list[FakeExternalWorkflowHandle] = []
    payload_converter = FakePayloadConverter()
    workflow_info = FakeWorkflowInfo(
        namespace="default",
        task_queue="current-task-queue",
    )

    signal_operation = output.WorkflowService.signal_with_start_workflow_execution
    retry_policy_operation = output.WorkflowService.retry_policy_operation
    activity_options_operation = output.WorkflowService.activity_options_operation
    registry = output.__nexus_operation_registry__
    signal_operation_key = (
        "WorkflowService",
        "SignalWithStartWorkflowExecution",
    )
    retry_policy_operation_key = (
        "WorkflowService",
        "RetryPolicyOperation",
    )
    activity_options_operation_key = (
        "WorkflowService",
        "ActivityOptionsOperation",
    )

    assert isinstance(signal_operation, Operation)
    assert isinstance(retry_policy_operation, Operation)
    assert isinstance(activity_options_operation, Operation)
    assert (
        signal_operation.name == "SignalWithStartWorkflowExecution"
    ), "unexpected operation name"
    assert (
        retry_policy_operation.name == "RetryPolicyOperation"
    ), "unexpected retry policy operation name"
    assert (
        activity_options_operation.name == "ActivityOptionsOperation"
    ), "unexpected activity options operation name"
    assert (
        registry[signal_operation_key] is signal_operation
    ), "registry entry should point at the signal operation"
    assert (
        registry[retry_policy_operation_key] is retry_policy_operation
    ), "registry entry should point at the retry policy operation"
    assert (
        registry[activity_options_operation_key] is activity_options_operation
    ), "registry entry should point at the activity options operation"
    assert hasattr(
        workflowservice_v1,
        "SignalWithStartWorkflowExecutionRequest",
    ), "request proto missing from installed temporalio package"
    assert hasattr(
        workflowservice_v1,
        "SignalWithStartWorkflowExecutionResponse",
    ), "response proto missing from installed temporalio package"
    assert hasattr(common_v1, "RetryPolicy"), "retry policy proto missing from installed temporalio package"
    assert hasattr(activity_v1, "ActivityOptions"), "activity options proto missing from installed temporalio package"

    retry_policy = temporal_common.RetryPolicy(
        initial_interval=timedelta(seconds=2),
        maximum_interval=timedelta(seconds=10),
        maximum_attempts=3,
    )
    search_attributes = temporal_common.TypedSearchAttributes(
        [
            temporal_common.SearchAttributePair(
                temporal_common.SearchAttributeKey.for_keyword("CustomKeyword"),
                "search-value",
            )
        ]
    )
    priority = temporal_common.Priority(
        priority_key=2,
        fairness_key="tenant-a",
        fairness_weight=3.5,
    )
    versioning_override = temporal_common.AutoUpgradeVersioningOverride()

    request = output.SignalWithStartWorkflowExecutionRequestModel(
        namespace="default",
        workflow_id="workflow-123",
        workflow_type=output.WorkflowTypeModel(name="ExampleWorkflow"),
        task_queue=output.TaskQueueModel(
            name="default",
            kind=output.TaskQueueKind.TASK_QUEUE_KIND_NORMAL,
        ),
        signal_name="wake_up",
        retry_policy=output.RetryPolicyModel(
            initial_interval=output.DurationModel(seconds=2, nanos=0),
            backoff_coefficient=2.0,
            maximum_interval=output.DurationModel(seconds=10, nanos=0),
            maximum_attempts=3,
        ),
    )
    request_proto = request.to_proto()
    assert isinstance(
        request_proto,
        workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
    )
    assert request_proto.namespace == "default"
    assert request_proto.workflow_id == "workflow-123"
    assert request_proto.workflow_type.name == "ExampleWorkflow"
    assert request_proto.task_queue.name == "default"
    assert (
        request_proto.task_queue.kind
        == output.TaskQueueKind.TASK_QUEUE_KIND_NORMAL
    )
    assert request_proto.signal_name == "wake_up"
    assert request_proto.retry_policy.maximum_attempts == 3
    assert request_proto.retry_policy.initial_interval.seconds == 2
    assert request_proto.retry_policy.maximum_interval.seconds == 10
    assert request_proto.retry_policy.backoff_coefficient == 2.0

    client = FakeNexusClient()
    original_create_nexus_client = workflow.create_nexus_client
    original_get_external_workflow_handle = workflow.get_external_workflow_handle
    original_info = workflow.info
    original_payload_converter = workflow.payload_converter

    def fake_create_nexus_client(
        *,
        service: type[object],
        endpoint: str,
    ) -> FakeNexusClient:
        created_clients.append((endpoint, service))
        return client

    def fake_get_external_workflow_handle(
        workflow_id: str,
        *,
        run_id: str | None = None,
    ) -> FakeExternalWorkflowHandle:
        handle = FakeExternalWorkflowHandle(workflow_id, run_id)
        created_handles.append(handle)
        return handle

    def fake_info() -> FakeWorkflowInfo:
        return workflow_info

    def fake_payload_converter() -> FakePayloadConverter:
        return payload_converter

    workflow.create_nexus_client = fake_create_nexus_client
    workflow.get_external_workflow_handle = fake_get_external_workflow_handle
    workflow.info = fake_info
    workflow.payload_converter = fake_payload_converter

    try:
        workflow_client = output.WorkflowServiceClient()
        response_handle = await workflow_client.signal_with_start_workflow_execution(
            request
        )
        response_proto = await response_handle
        response = (
            output.WorkflowServiceClient.signal_with_start_workflow_execution_response_from_proto(
                response_proto
            )
        )
        ergonomic_handle = await workflow_client.signal_with_start_workflow(
            ExampleWorkflow.run,
            ExampleWorkflow.wake_up,
            id="workflow-123",
            signal_args=["signal-arg", 42],
            workflow_args=["workflow-arg"],
            task_queue="default",
            execution_timeout=timedelta(minutes=5),
            run_timeout=timedelta(minutes=2),
            task_timeout=timedelta(seconds=30),
            id_reuse_policy=temporal_common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY,
            id_conflict_policy=temporal_common.WorkflowIDConflictPolicy.USE_EXISTING,
            retry_policy=retry_policy,
            cron_schedule="*/5 * * * *",
            memo={"memo-key": "memo-value"},
            search_attributes=search_attributes,
            static_summary="summary",
            static_details="details",
            start_delay=timedelta(seconds=7),
            request_id="request-123",
            priority=priority,
            versioning_override=versioning_override,
        )
        defaulted_handle = await workflow_client.signal_with_start_workflow(
            ExampleWorkflow.run,
            ExampleWorkflow.wake_up,
            id="workflow-123",
        )
        round_tripped_retry_policy = await workflow_client.round_trip_retry_policy(
            retry_policy
        )
        round_tripped_activity_options = (
            await workflow_client.round_trip_activity_options(
                task_queue="activity-queue",
                schedule_to_close_timeout=timedelta(seconds=20),
                schedule_to_start_timeout=timedelta(seconds=5),
                start_to_close_timeout=timedelta(seconds=15),
                heartbeat_timeout=timedelta(seconds=3),
                retry_policy=retry_policy,
                priority=priority,
            )
        )
    finally:
        workflow.create_nexus_client = original_create_nexus_client
        workflow.get_external_workflow_handle = original_get_external_workflow_handle
        workflow.info = original_info
        workflow.payload_converter = original_payload_converter

    assert created_clients == [("__temporal_system", output.WorkflowService)]
    assert len(client.calls) == 5
    assert client.calls[0][0] is signal_operation
    assert client.calls[1][0] is signal_operation
    assert client.calls[2][0] is signal_operation
    assert client.calls[3][0] is retry_policy_operation
    assert client.calls[4][0] is activity_options_operation
    first_input = client.calls[0][1]
    second_input = client.calls[1][1]
    third_input = client.calls[2][1]
    fourth_input = client.calls[3][1]
    fifth_input = client.calls[4][1]
    assert isinstance(first_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert isinstance(second_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert isinstance(third_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert isinstance(fourth_input, common_v1.RetryPolicy)
    assert isinstance(fifth_input, activity_v1.ActivityOptions)
    assert first_input.retry_policy.maximum_attempts == 3
    assert first_input.retry_policy.initial_interval.seconds == 2
    assert first_input.retry_policy.maximum_interval.seconds == 10
    assert first_input.retry_policy.backoff_coefficient == 2.0
    assert (
        first_input.task_queue.kind
        == output.TaskQueueKind.TASK_QUEUE_KIND_NORMAL
    )
    assert second_input.task_queue.kind == 0
    assert second_input.task_queue.name == "default"
    assert second_input.request_id == "request-123"
    assert len(second_input.signal_input.payloads) == 2
    assert second_input.signal_input.payloads[0].data == b"str:'signal-arg'"
    assert second_input.signal_input.payloads[1].data == b'int:42'
    assert len(second_input.input.payloads) == 1
    assert second_input.input.payloads[0].data == b"str:'workflow-arg'"
    assert second_input.workflow_execution_timeout.seconds == 300
    assert second_input.workflow_run_timeout.seconds == 120
    assert second_input.workflow_task_timeout.seconds == 30
    assert (
        second_input.workflow_id_reuse_policy
        == temporal_common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY
    )
    assert (
        second_input.workflow_id_conflict_policy
        == temporal_common.WorkflowIDConflictPolicy.USE_EXISTING
    )
    assert second_input.retry_policy.maximum_attempts == 3
    assert second_input.retry_policy.initial_interval.seconds == 2
    assert second_input.retry_policy.maximum_interval.seconds == 10
    assert second_input.retry_policy.backoff_coefficient == 2.0
    assert second_input.cron_schedule == "*/5 * * * *"
    assert second_input.memo.fields["memo-key"].data == b"str:'memo-value'"
    assert (
        second_input.search_attributes.indexed_fields["CustomKeyword"]
        == temporalio.converter.encode_typed_search_attribute_value(
            temporal_common.SearchAttributeKey.for_keyword("CustomKeyword"),
            "search-value",
        )
    )
    assert second_input.user_metadata.summary.data == b"str:'summary'"
    assert second_input.user_metadata.details.data == b"str:'details'"
    assert second_input.workflow_start_delay.seconds == 7
    assert second_input.priority.priority_key == 2
    assert second_input.priority.fairness_key == "tenant-a"
    assert second_input.priority.fairness_weight == 3.5
    assert second_input.versioning_override.auto_upgrade is True
    assert not third_input.HasField("retry_policy")
    assert third_input.task_queue.kind == 0
    assert third_input.task_queue.name == "current-task-queue"
    assert len(third_input.signal_input.payloads) == 0
    assert len(third_input.input.payloads) == 0
    assert (
        third_input.workflow_id_reuse_policy
        == temporal_common.WorkflowIDReusePolicy.ALLOW_DUPLICATE
    )
    assert (
        third_input.workflow_id_conflict_policy
        == temporal_common.WorkflowIDConflictPolicy.UNSPECIFIED
    )
    assert third_input.cron_schedule == ""
    assert third_input.HasField("priority")
    assert fourth_input.initial_interval.seconds == 2
    assert fourth_input.maximum_interval.seconds == 10
    assert fourth_input.maximum_attempts == 3
    assert fourth_input.backoff_coefficient == 2.0
    assert fifth_input.task_queue.name == "activity-queue"
    assert fifth_input.schedule_to_close_timeout.seconds == 20
    assert fifth_input.schedule_to_start_timeout.seconds == 5
    assert fifth_input.start_to_close_timeout.seconds == 15
    assert fifth_input.heartbeat_timeout.seconds == 3
    assert fifth_input.retry_policy.maximum_attempts == 3
    assert fifth_input.retry_policy.initial_interval.seconds == 2
    assert fifth_input.priority.priority_key == 2
    assert fifth_input.priority.fairness_key == "tenant-a"
    assert fifth_input.priority.fairness_weight == 3.5
    assert response.run_id == "run-123"
    assert response.started is True
    assert len(created_handles) == 2
    assert ergonomic_handle is created_handles[0]
    assert defaulted_handle is created_handles[1]
    assert ergonomic_handle.id == "workflow-123"
    assert ergonomic_handle.run_id == "run-123"
    assert defaulted_handle.id == "workflow-123"
    assert defaulted_handle.run_id == "run-123"
    assert round_tripped_retry_policy is not None
    assert round_tripped_retry_policy.initial_interval == timedelta(seconds=2)
    assert round_tripped_retry_policy.maximum_interval == timedelta(seconds=10)
    assert round_tripped_retry_policy.maximum_attempts == 3
    assert round_tripped_retry_policy.backoff_coefficient == 2.0
    assert round_tripped_activity_options.get("task_queue") == "activity-queue"
    assert round_tripped_activity_options.get("schedule_to_close_timeout") == timedelta(
        seconds=20
    )
    assert round_tripped_activity_options.get("schedule_to_start_timeout") == timedelta(
        seconds=5
    )
    assert round_tripped_activity_options.get("start_to_close_timeout") == timedelta(
        seconds=15
    )
    assert round_tripped_activity_options.get("heartbeat_timeout") == timedelta(
        seconds=3
    )
    activity_retry_policy = round_tripped_activity_options.get("retry_policy")
    assert activity_retry_policy is not None
    assert activity_retry_policy.initial_interval == timedelta(seconds=2)
    assert activity_retry_policy.maximum_interval == timedelta(seconds=10)
    assert activity_retry_policy.maximum_attempts == 3
    activity_priority = round_tripped_activity_options.get("priority")
    assert activity_priority is not None
    assert activity_priority.priority_key == 2
    assert activity_priority.fairness_key == "tenant-a"
    assert activity_priority.fairness_weight == 3.5

    print(f"Validated generated module: {OUTPUT_PATH}")
    print(f"Operation registry contains {len(registry)} entry")
    print(
        "Recursive dataclass conversion, generated enum wrappers, ergonomic API conversion, defaulted parameters, and the start_operation wrapper look correct"
    )


if __name__ == "__main__":
    asyncio.run(main())
