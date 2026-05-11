from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable, Generator, Sequence
import datetime
from pathlib import Path
import typing

from nexusrpc import Operation
import temporalio.api.activity.v1 as activity_v1
import temporalio.api.common.v1
import temporalio.api.common.v1.message_pb2 as common_pb2
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.common
import temporalio.workflow

import output

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT / "output.py"


ResponseProto = (
    activity_v1.ActivityOptions
    | temporalio.api.common.v1.RetryPolicy
    | workflowservice_v1.SignalWithStartWorkflowExecutionResponse
)


@temporalio.workflow.defn
class ExampleWorkflow:
    @temporalio.workflow.run
    async def run(self, attempt: int, name: str) -> str:
        return f"{attempt}:{name}"


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


class FakePayloadConverter:
    @staticmethod
    def _encode(value: object) -> bytes:
        return f"{type(value).__name__}:{value!r}".encode()

    @staticmethod
    def _decode(data: bytes) -> object:
        return data.decode()

    def to_payloads_wrapper(
        self,
        values: Sequence[object],
    ) -> common_pb2.Payloads:
        message = common_pb2.Payloads()
        for value in values:
            payload = message.payloads.add()
            payload.data = self._encode(value)
        return message

    def from_payloads_wrapper(
        self,
        payloads: common_pb2.Payloads | None,
    ) -> list[object]:
        if not payloads:
            return []
        return [self._decode(payload.data) for payload in payloads.payloads]


class FakeNexusClient:
    def __init__(self) -> None:
        self.calls: list[tuple[Operation[object, object], object]] = []

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
            assert input.workflow_id == "workflow-123"
            assert input.signal_name == "wake_up"
            assert input.workflow_type.name == "ExampleWorkflow"
            assert input.task_queue.name == "demo-task-queue"
            assert input.HasField("input")
            assert [payload.data for payload in input.input.payloads] == [
                b"int:7",
                b"str:'nexus'",
            ]
            assert input.workflow_execution_timeout.seconds == 30
            assert input.retry_policy.maximum_attempts == 3
            assert input.workflow_id_reuse_policy == int(
                temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY
            )
            assert input.workflow_id_conflict_policy == int(
                temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING
            )
            assert input.priority.priority_key == 4
            assert input.priority.fairness_key == "tenant-a"
            assert input.priority.fairness_weight == 2.5
            assert input.memo.fields["category"].data == b"str:'payments'"
            assert input.memo.fields["attempt"].data == b"int:7"
            assert "CustomKeywordField" in input.search_attributes.indexed_fields
            assert input.user_metadata.summary.data == b"str:'Nightly sync'"
            assert input.user_metadata.details.data == b"str:'Processes 42 records'"
            assert input.versioning_override.HasField("pinned")
            assert input.versioning_override.pinned.version.deployment_name == "payments"
            assert input.versioning_override.pinned.version.build_id == "build-42"

            response = workflowservice_v1.SignalWithStartWorkflowExecutionResponse()
            response.run_id = "run-123"
            response.started = True
            return FakeOperationHandle(response)

        if operation is output.WorkflowService.retry_policy_operation:
            assert isinstance(input, temporalio.api.common.v1.RetryPolicy)
            response = temporalio.api.common.v1.RetryPolicy()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        if operation is output.WorkflowService.activity_options_operation:
            assert isinstance(input, activity_v1.ActivityOptions)
            assert input.HasField("retry_policy")
            assert input.task_queue.name == "demo-task-queue"
            assert input.schedule_to_close_timeout.seconds == 7
            assert input.priority.priority_key == 4
            response = activity_v1.ActivityOptions()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        raise AssertionError(f"unexpected operation: {operation.name}")


def assert_missing_required_field(
    owner_and_field: str,
    build_proto: Callable[[], object],
) -> None:
    try:
        _ = build_proto()
    except ValueError as error:
        assert str(error) == f"missing required field {owner_and_field}"
    else:
        raise AssertionError(f"expected required field validation for {owner_and_field}")


def make_signal_request(
    *,
    workflow_type: str | Callable[..., Awaitable[typing.Any]] = ExampleWorkflow.run,
    workflow_id: str = "workflow-123",
    task_queue: str = "demo-task-queue",
    signal_name: str = "wake_up",
) -> output.SignalWithStartWorkflowExecutionRequest:
    return output.SignalWithStartWorkflowExecutionRequest(
        workflow_type=workflow_type,
        workflow_id=workflow_id,
        task_queue=task_queue,
        signal_name=signal_name,
    )


async def main() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated file at {OUTPUT_PATH}"

    signal_operation = output.WorkflowService.signal_with_start_workflow_execution
    retry_operation = output.WorkflowService.retry_policy_operation
    activity_operation = output.WorkflowService.activity_options_operation
    registry = output.__nexus_operation_registry__

    assert isinstance(signal_operation, Operation)
    assert isinstance(retry_operation, Operation)
    assert isinstance(activity_operation, Operation)
    assert registry[("WorkflowService", "SignalWithStartWorkflowExecution")] is signal_operation
    assert registry[("WorkflowService", "RetryPolicyOperation")] is retry_operation
    assert registry[("WorkflowService", "ActivityOptionsOperation")] is activity_operation
    assert hasattr(output, "SignalWithStartWorkflowExecutionRequest")
    assert not hasattr(output.SignalWithStartWorkflowExecutionRequest, "from_proto")
    assert not hasattr(output, "SignalWithStartWorkflowExecutionRequestTyped")
    assert not hasattr(output, "RetryPolicy")
    assert not hasattr(output, "WorkflowType")
    assert not hasattr(output, "TaskQueue")
    assert not hasattr(output, "Payload")
    assert not hasattr(output, "ExternalPayloadDetails")
    assert not hasattr(output, "Payloads")
    assert not hasattr(output, "Memo")
    assert not hasattr(output, "Header")
    assert not hasattr(output, "SearchAttributes")
    assert hasattr(output, "UserMetadata")
    assert not hasattr(output, "WorkflowIdReusePolicy")
    assert not hasattr(output, "WorkflowIdConflictPolicy")
    assert not hasattr(output, "Link")
    assert not hasattr(output, "Priority")
    assert not hasattr(output, "VersioningOverride")

    retry_policy = temporalio.common.RetryPolicy(maximum_attempts=3)
    priority = temporalio.common.Priority(
        priority_key=4,
        fairness_key="tenant-a",
        fairness_weight=2.5,
    )
    versioning_override = temporalio.common.PinnedVersioningOverride(
        temporalio.common.WorkerDeploymentVersion(
            deployment_name="payments",
            build_id="build-42",
        )
    )
    search_key = temporalio.common.SearchAttributeKey.for_keyword("CustomKeywordField")
    typed_search_attributes = temporalio.common.TypedSearchAttributes(
        [temporalio.common.SearchAttributePair(search_key, "sample-value")]
    )

    assert_missing_required_field(
        "ActivityOptions.retry_policy",
        lambda: output.ActivityOptions(
            retry_policy=typing.cast(
                temporalio.common.RetryPolicy,
                typing.cast(object, None),
            )
        ).to_proto(),
    )
    assert_missing_required_field(
        "SignalWithStartWorkflowExecutionRequest.workflow_type",
        lambda: make_signal_request(
            workflow_type=typing.cast(str, typing.cast(object, None))
        ).to_proto(),
    )
    assert_missing_required_field(
        "SignalWithStartWorkflowExecutionRequest.workflow_id",
        lambda: make_signal_request(
            workflow_id=typing.cast(str, typing.cast(object, None))
        ).to_proto(),
    )
    assert_missing_required_field(
        "SignalWithStartWorkflowExecutionRequest.task_queue",
        lambda: make_signal_request(
            task_queue=typing.cast(str, typing.cast(object, None))
        ).to_proto(),
    )
    assert_missing_required_field(
        "SignalWithStartWorkflowExecutionRequest.signal_name",
        lambda: make_signal_request(
            signal_name=typing.cast(str, typing.cast(object, None))
        ).to_proto(),
    )
    activity_options = output.ActivityOptions(
        retry_policy=retry_policy,
        task_queue="demo-task-queue",
        priority=priority,
    )
    activity_options.schedule_to_close_timeout = datetime.timedelta(seconds=7)
    activity_proto = activity_options.to_proto()
    assert activity_proto.HasField("retry_policy")
    assert activity_proto.task_queue.name == "demo-task-queue"
    assert activity_proto.schedule_to_close_timeout.seconds == 7
    assert activity_proto.priority.priority_key == 4
    round_tripped_activity = output.ActivityOptions.from_proto(activity_proto)
    assert isinstance(round_tripped_activity.retry_policy, temporalio.common.RetryPolicy)
    assert round_tripped_activity.retry_policy.maximum_attempts == 3
    assert round_tripped_activity.task_queue == "demo-task-queue"
    assert round_tripped_activity.schedule_to_close_timeout == datetime.timedelta(seconds=7)
    assert round_tripped_activity.priority == priority

    fake_client = FakeNexusClient()
    fake_payload_converter = FakePayloadConverter()
    created_clients: list[tuple[type[object], str]] = []

    def fake_create_nexus_client(*, service: type[object], endpoint: str) -> FakeNexusClient:
        created_clients.append((service, endpoint))
        return fake_client

    def fake_workflow_payload_converter() -> FakePayloadConverter:
        return fake_payload_converter

    workflow_module = output.workflow  # pyright: ignore[reportPrivateLocalImportUsage]
    setattr(workflow_module, "create_nexus_client", fake_create_nexus_client)
    setattr(workflow_module, "payload_converter", fake_workflow_payload_converter)
    client = output.WorkflowServiceClient()

    assert created_clients == [(output.WorkflowService, "__temporal_system")]

    request: output.SignalWithStartWorkflowExecutionRequest = output.SignalWithStartWorkflowExecutionRequest(
        workflow_type=ExampleWorkflow.run,
        workflow_id="workflow-123",
        task_queue="demo-task-queue",
        signal_name="wake_up",
        input=(7, "nexus"),
        workflow_execution_timeout=datetime.timedelta(seconds=30),
        retry_policy=retry_policy,
        workflow_id_reuse_policy=temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY,
        workflow_id_conflict_policy=temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING,
        memo={"category": "payments", "attempt": 7},
        search_attributes=typed_search_attributes,
        user_metadata=output.UserMetadata(
            summary="Nightly sync",
            details="Processes 42 records",
        ),
        priority=priority,
        versioning_override=versioning_override,
    )
    assert "header" not in request.__dataclass_fields__
    assert "links" not in request.__dataclass_fields__
    request_proto = request.to_proto()
    assert request_proto.workflow_type.name == "ExampleWorkflow"
    assert request_proto.workflow_id == "workflow-123"
    assert request_proto.task_queue.name == "demo-task-queue"
    assert [payload.data for payload in request_proto.input.payloads] == [
        b"int:7",
        b"str:'nexus'",
    ]
    assert request_proto.workflow_execution_timeout.seconds == 30
    assert request_proto.retry_policy.maximum_attempts == 3
    assert request_proto.workflow_id_reuse_policy == int(
        temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY
    )
    assert request_proto.workflow_id_conflict_policy == int(
        temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING
    )
    assert request_proto.priority.priority_key == 4
    assert request_proto.memo.fields["category"].data == b"str:'payments'"
    assert request_proto.memo.fields["attempt"].data == b"int:7"
    assert "CustomKeywordField" in request_proto.search_attributes.indexed_fields
    assert request_proto.user_metadata.summary.data == b"str:'Nightly sync'"
    assert request_proto.user_metadata.details.data == b"str:'Processes 42 records'"
    round_tripped_user_metadata = output.UserMetadata.from_proto(
        request_proto.user_metadata
    )
    assert round_tripped_user_metadata.summary == "str:'Nightly sync'"
    assert round_tripped_user_metadata.details == "str:'Processes 42 records'"
    assert request_proto.versioning_override.pinned.version.deployment_name == "payments"
    assert len(request_proto.links) == 0

    signal_handle = await client.signal_with_start_workflow_execution(request)
    signal_response = output.SignalWithStartWorkflowExecutionResponse.from_proto(
        await signal_handle
    )
    assert signal_response.run_id == "run-123"
    assert signal_response.started is True

    retry_handle = await client.retry_policy_operation(retry_policy)
    retry_round_trip = output.retry_policy_from_proto(await retry_handle)
    assert retry_round_trip.maximum_attempts == 3

    activity_handle = await client.activity_options_operation(activity_options)
    activity_response = output.ActivityOptions.from_proto(await activity_handle)
    assert isinstance(activity_response.retry_policy, temporalio.common.RetryPolicy)
    assert activity_response.retry_policy.maximum_attempts == 3
    assert activity_response.task_queue == "demo-task-queue"
    assert activity_response.schedule_to_close_timeout == datetime.timedelta(seconds=7)
    assert activity_response.priority == priority

    assert len(fake_client.calls) == 3
    print(
        "Generated Python-native model overrides, payload encoding helpers, required-field validation, low-level operation wrappers, and registry metadata look correct"
    )


if __name__ == "__main__":
    asyncio.run(main())
