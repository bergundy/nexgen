from __future__ import annotations

from collections.abc import Awaitable, Callable, Generator, Sequence
import dataclasses
import datetime
from pathlib import Path
import sys
import typing

from nexusrpc import Operation
import pytest
import temporalio.api.activity.v1 as activity_v1
import temporalio.api.common.v1
import temporalio.api.common.v1.message_pb2 as common_pb2
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.common
import temporalio.workflow

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT / "workflow_service"

if str(APP_ROOT) not in sys.path:
    sys.path.insert(0, str(APP_ROOT))

import workflow_service as output
import workflow_service.models as output_models
import workflow_service._support as output_support

SIGNAL_WITH_START_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "SignalWithStartWorkflowExecution")
]
RETRY_POLICY_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "RetryPolicyOperation")
]
ACTIVITY_OPTIONS_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "ActivityOptionsOperation")
]

TASK_QUEUE = "demo-task-queue"

REQUEST_WORKFLOW_ID = "workflow-request"
ARGS_WORKFLOW_ID = "workflow-args"
MINIMAL_WORKFLOW_ID = "workflow-minimal"
HIGH_ARITY_WORKFLOW_ID = "workflow-high-arity"

FULL_WORKFLOW_INPUT = (7, "nexus")
ARGS_SIGNAL_INPUT = ("wake-up",)
HIGH_ARITY_SIGNAL_INPUT = (
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
)


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

    @temporalio.workflow.signal
    def wake_up(self, reason: str) -> None:
        _ = reason

    @temporalio.workflow.signal
    def wake_up_many(
        self,
        first: str,
        second: str,
        third: str,
        fourth: str,
        fifth: str,
        sixth: str,
        seventh: str,
    ) -> None:
        _ = (first, second, third, fourth, fifth, sixth, seventh)


@dataclasses.dataclass(frozen=True)
class ExampleData:
    retry_policy: temporalio.common.RetryPolicy
    priority: temporalio.common.Priority
    versioning_override: temporalio.common.VersioningOverride
    typed_search_attributes: temporalio.common.TypedSearchAttributes


@dataclasses.dataclass
class ClientContext:
    fake_client: FakeNexusClient
    created_clients: list[tuple[type[object], str]]
    created_external_handles: list[FakeExternalWorkflowHandle]


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


class FakeExternalWorkflowHandle:
    id: str
    run_id: str | None

    def __init__(self, workflow_id: str, run_id: str | None) -> None:
        self.id = workflow_id
        self.run_id = run_id

    async def signal(self, signal: object, *args: object) -> None:
        _ = signal
        _ = args

    async def cancel(self) -> None:
        return None


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

        if operation is SIGNAL_WITH_START_OPERATION:
            assert isinstance(
                input,
                workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
            )
            if input.workflow_id == REQUEST_WORKFLOW_ID:
                assert_full_signal_request(
                    input,
                    workflow_id=REQUEST_WORKFLOW_ID,
                    signal_name="wake_up",
                    signal_input=None,
                )
            elif input.workflow_id == ARGS_WORKFLOW_ID:
                assert_full_signal_request(
                    input,
                    workflow_id=ARGS_WORKFLOW_ID,
                    signal_name="wake_up",
                    signal_input=ARGS_SIGNAL_INPUT,
                )
            elif input.workflow_id == HIGH_ARITY_WORKFLOW_ID:
                assert_full_signal_request(
                    input,
                    workflow_id=HIGH_ARITY_WORKFLOW_ID,
                    signal_name="wake_up_many",
                    signal_input=HIGH_ARITY_SIGNAL_INPUT,
                )
            elif input.workflow_id == MINIMAL_WORKFLOW_ID:
                assert_minimal_signal_request(input)
            else:
                raise AssertionError(f"unexpected signal-with-start workflow id: {input.workflow_id}")

            response = workflowservice_v1.SignalWithStartWorkflowExecutionResponse()
            response.run_id = expected_run_id(input.workflow_id)
            response.started = True
            return FakeOperationHandle(response)

        if operation is RETRY_POLICY_OPERATION:
            assert isinstance(input, temporalio.api.common.v1.RetryPolicy)
            response = temporalio.api.common.v1.RetryPolicy()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        if operation is ACTIVITY_OPTIONS_OPERATION:
            assert isinstance(input, activity_v1.ActivityOptions)
            assert input.HasField("retry_policy")
            assert input.task_queue.name == TASK_QUEUE
            assert input.schedule_to_close_timeout.seconds == 7
            assert input.priority.priority_key == 4
            response = activity_v1.ActivityOptions()
            response.CopyFrom(input)
            return FakeOperationHandle(response)

        raise AssertionError(f"unexpected operation: {operation.name}")


def expected_run_id(workflow_id: str) -> str:
    return f"run-for-{workflow_id}"


def expected_payload_bytes(values: Sequence[object]) -> list[bytes]:
    return [f"{type(value).__name__}:{value!r}".encode() for value in values]


def assert_payload_values(
    payloads: common_pb2.Payloads,
    expected_values: Sequence[object],
) -> None:
    assert [payload.data for payload in payloads.payloads] == expected_payload_bytes(
        expected_values
    )


def assert_common_signal_request(
    request: workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
    *,
    workflow_id: str,
    signal_name: str,
) -> None:
    assert request.namespace == "workflow-namespace"
    assert request.workflow_id == workflow_id
    assert request.signal_name == signal_name
    assert request.workflow_type.name == "ExampleWorkflow"
    assert request.task_queue.name == TASK_QUEUE


def assert_full_signal_request(
    request: workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
    *,
    workflow_id: str,
    signal_name: str,
    signal_input: Sequence[object] | None,
) -> None:
    assert_common_signal_request(
        request,
        workflow_id=workflow_id,
        signal_name=signal_name,
    )
    assert request.HasField("input")
    assert_payload_values(request.input, FULL_WORKFLOW_INPUT)
    if signal_input is None:
        assert not request.HasField("signal_input")
    else:
        assert request.HasField("signal_input")
        assert_payload_values(request.signal_input, signal_input)
    assert request.workflow_execution_timeout.seconds == 30
    assert request.retry_policy.maximum_attempts == 3
    assert request.workflow_id_reuse_policy == int(
        temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY
    )
    assert request.workflow_id_conflict_policy == int(
        temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING
    )
    assert request.priority.priority_key == 4
    assert request.priority.fairness_key == "tenant-a"
    assert request.priority.fairness_weight == 2.5
    assert request.memo.fields["category"].data == b"str:'payments'"
    assert request.memo.fields["attempt"].data == b"int:7"
    assert "CustomKeywordField" in request.search_attributes.indexed_fields
    assert request.user_metadata.summary.data == b"str:'Nightly sync'"
    assert request.user_metadata.details.data == b"str:'Processes 42 records'"
    assert request.versioning_override.HasField("pinned")
    assert request.versioning_override.pinned.version.deployment_name == "payments"
    assert request.versioning_override.pinned.version.build_id == "build-42"


def assert_minimal_signal_request(
    request: workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
) -> None:
    assert_common_signal_request(
        request,
        workflow_id=MINIMAL_WORKFLOW_ID,
        signal_name="wake_up",
    )
    assert not request.HasField("input")
    assert not request.HasField("workflow_execution_timeout")
    assert not request.HasField("retry_policy")
    assert request.workflow_id_reuse_policy == 0
    assert request.workflow_id_conflict_policy == 0
    assert not request.HasField("priority")
    assert len(request.memo.fields) == 0
    assert len(request.search_attributes.indexed_fields) == 0
    assert not request.HasField("user_metadata")
    assert not request.HasField("versioning_override")
    assert not request.HasField("signal_input")


def build_example_data() -> ExampleData:
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
    return ExampleData(
        retry_policy=retry_policy,
        priority=priority,
        versioning_override=versioning_override,
        typed_search_attributes=typed_search_attributes,
    )


def build_full_signal_request(
    example_data: ExampleData,
    *,
    workflow_id: str,
    signal: str | Callable[..., None | Awaitable[None]],
    signal_input: tuple[typing.Any, ...] | None = None,
) -> typing.Any:
    return output_models.SignalWithStartWorkflowExecutionRequest(
        workflow=ExampleWorkflow.run,
        workflow_id=workflow_id,
        task_queue=TASK_QUEUE,
        signal=signal,
        input=FULL_WORKFLOW_INPUT,
        workflow_execution_timeout=datetime.timedelta(seconds=30),
        retry_policy=example_data.retry_policy,
        workflow_id_reuse_policy=temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY,
        workflow_id_conflict_policy=temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING,
        signal_input=signal_input,
        memo={"category": "payments", "attempt": 7},
        search_attributes=example_data.typed_search_attributes,
        user_metadata=output_models.UserMetadata(
            static_summary="Nightly sync",
            static_details="Processes 42 records",
        ),
        priority=example_data.priority,
        versioning_override=example_data.versioning_override,
    )


def make_signal_request(
    *,
    workflow: str | Callable[..., Awaitable[object]] = ExampleWorkflow.run,
    workflow_id: str = REQUEST_WORKFLOW_ID,
    task_queue: str = TASK_QUEUE,
    signal: str | Callable[..., None | Awaitable[None]] = "wake_up",
) -> typing.Any:
    return output_models.SignalWithStartWorkflowExecutionRequest(
        workflow=workflow,
        workflow_id=workflow_id,
        task_queue=task_queue,
        signal=signal,
    )


def build_activity_options(example_data: ExampleData) -> typing.Any:
    activity_options = output_models.ActivityOptions(
        retry_policy=example_data.retry_policy,
        task_queue=TASK_QUEUE,
        priority=example_data.priority,
    )
    activity_options.schedule_to_close_timeout = datetime.timedelta(seconds=7)
    return activity_options


@pytest.fixture
def example_data() -> ExampleData:
    return build_example_data()


@pytest.fixture
def context() -> ClientContext:
    return install_fake_runtime()


def assert_handle_matches(
    handle: temporalio.workflow.ExternalWorkflowHandle[typing.Any],
    workflow_id: str,
) -> None:
    _ = typing.assert_type(
        handle,
        temporalio.workflow.ExternalWorkflowHandle[typing.Any],
    )
    assert handle.id == workflow_id
    assert handle.run_id == expected_run_id(workflow_id)


def install_fake_runtime() -> ClientContext:
    fake_client = FakeNexusClient()
    fake_payload_converter = FakePayloadConverter()
    created_clients: list[tuple[type[object], str]] = []
    created_external_handles: list[FakeExternalWorkflowHandle] = []

    def fake_create_nexus_client(*, service: type[object], endpoint: str) -> FakeNexusClient:
        created_clients.append((service, endpoint))
        return fake_client

    def fake_workflow_payload_converter() -> FakePayloadConverter:
        return fake_payload_converter

    class FakeWorkflowInfo:
        namespace: str = "workflow-namespace"

    def fake_workflow_info() -> FakeWorkflowInfo:
        return FakeWorkflowInfo()

    def fake_get_external_workflow_handle(
        workflow_id: str,
        *,
        run_id: str | None = None,
    ) -> FakeExternalWorkflowHandle:
        handle = FakeExternalWorkflowHandle(workflow_id, run_id)
        created_external_handles.append(handle)
        return handle

    workflow_module = temporalio.workflow
    setattr(workflow_module, "create_nexus_client", fake_create_nexus_client)
    setattr(workflow_module, "payload_converter", fake_workflow_payload_converter)
    setattr(workflow_module, "info", fake_workflow_info)
    setattr(
        workflow_module,
        "get_external_workflow_handle",
        fake_get_external_workflow_handle,
    )

    return ClientContext(
        fake_client=fake_client,
        created_clients=created_clients,
        created_external_handles=created_external_handles,
    )


def test_generated_metadata() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated package at {OUTPUT_PATH}"
    signal_operation = SIGNAL_WITH_START_OPERATION
    retry_operation = RETRY_POLICY_OPERATION
    activity_operation = ACTIVITY_OPTIONS_OPERATION
    registry = output.__nexus_operation_registry__

    assert isinstance(signal_operation, Operation)
    assert isinstance(retry_operation, Operation)
    assert isinstance(activity_operation, Operation)
    assert registry[("WorkflowService", "SignalWithStartWorkflowExecution")] is signal_operation
    assert registry[("WorkflowService", "RetryPolicyOperation")] is retry_operation
    assert registry[("WorkflowService", "ActivityOptionsOperation")] is activity_operation
    assert not hasattr(output, "WorkflowService")
    assert not hasattr(output, "SignalWithStartWorkflowExecutionRequest")
    assert not hasattr(output, "UserMetadata")
    assert not hasattr(output, "ActivityOptions")
    assert not hasattr(output, "workflow")


def test_activity_options_round_trip(example_data: ExampleData) -> None:
    activity_options = build_activity_options(example_data)
    activity_proto = activity_options.to_proto()
    assert activity_proto.HasField("retry_policy")
    assert activity_proto.task_queue.name == TASK_QUEUE
    assert activity_proto.schedule_to_close_timeout.seconds == 7
    assert activity_proto.priority.priority_key == 4
    round_tripped_activity = output_models.ActivityOptions.from_proto(activity_proto)
    assert isinstance(round_tripped_activity.retry_policy, temporalio.common.RetryPolicy)
    assert round_tripped_activity.retry_policy.maximum_attempts == 3
    assert round_tripped_activity.task_queue == TASK_QUEUE
    assert round_tripped_activity.schedule_to_close_timeout == datetime.timedelta(seconds=7)
    assert round_tripped_activity.priority == example_data.priority


@pytest.mark.asyncio
async def test_signal_request_api(
    context: ClientContext,
    example_data: ExampleData,
) -> None:
    request = build_full_signal_request(
        example_data,
        workflow_id=REQUEST_WORKFLOW_ID,
        signal="wake_up",
    )
    assert "header" not in request.__dataclass_fields__
    assert "links" not in request.__dataclass_fields__
    assert "namespace" not in request.__dataclass_fields__

    request_proto = request.to_proto()
    assert_full_signal_request(
        request_proto,
        workflow_id=REQUEST_WORKFLOW_ID,
        signal_name="wake_up",
        signal_input=None,
    )
    round_tripped_user_metadata = output_models.UserMetadata.from_proto(
        request_proto.user_metadata
    )
    assert round_tripped_user_metadata.static_summary == "str:'Nightly sync'"
    assert round_tripped_user_metadata.static_details == "str:'Processes 42 records'"
    assert len(request_proto.links) == 0

@pytest.mark.asyncio
async def test_signal_args_api(
    context: ClientContext,
    example_data: ExampleData,
) -> None:
    handle = await output.signal_with_start_workflow_execution(
        workflow=ExampleWorkflow.run,
        workflow_id=ARGS_WORKFLOW_ID,
        task_queue=TASK_QUEUE,
        signal=ExampleWorkflow.wake_up,
        input=FULL_WORKFLOW_INPUT,
        signal_input="wake-up",
        workflow_execution_timeout=datetime.timedelta(seconds=30),
        retry_policy=example_data.retry_policy,
        workflow_id_reuse_policy=temporalio.common.WorkflowIDReusePolicy.ALLOW_DUPLICATE_FAILED_ONLY,
        workflow_id_conflict_policy=temporalio.common.WorkflowIDConflictPolicy.TERMINATE_EXISTING,
        memo={"category": "payments", "attempt": 7},
        search_attributes=example_data.typed_search_attributes,
        static_summary="Nightly sync",
        static_details="Processes 42 records",
        priority=example_data.priority,
        versioning_override=example_data.versioning_override,
    )
    assert_handle_matches(handle, ARGS_WORKFLOW_ID)


@pytest.mark.asyncio
async def test_signal_minimal_args_api(context: ClientContext) -> None:
    handle = await output.signal_with_start_workflow_execution(
        workflow="ExampleWorkflow",
        workflow_id=MINIMAL_WORKFLOW_ID,
        task_queue=TASK_QUEUE,
        signal="wake_up",
    )
    assert_handle_matches(handle, MINIMAL_WORKFLOW_ID)


@pytest.mark.asyncio
async def test_signal_high_arity_request_api(
    context: ClientContext,
    example_data: ExampleData,
) -> None:
    request = build_full_signal_request(
        example_data,
        workflow_id=HIGH_ARITY_WORKFLOW_ID,
        signal=ExampleWorkflow.wake_up_many,
        signal_input=HIGH_ARITY_SIGNAL_INPUT,
    )
    request_proto = request.to_proto()
    assert_full_signal_request(
        request_proto,
        workflow_id=HIGH_ARITY_WORKFLOW_ID,
        signal_name="wake_up_many",
        signal_input=HIGH_ARITY_SIGNAL_INPUT,
    )

@pytest.mark.asyncio
async def test_retry_policy_operation(
    context: ClientContext,
    example_data: ExampleData,
) -> None:
    retry_handle = await output.retry_policy_operation(example_data.retry_policy)
    retry_round_trip = output_support.retry_policy_from_proto(await retry_handle)
    assert retry_round_trip.maximum_attempts == 3


@pytest.mark.asyncio
async def test_activity_options_operation(
    context: ClientContext,
    example_data: ExampleData,
) -> None:
    activity_handle = await output.activity_options_operation(
        task_queue=TASK_QUEUE,
        schedule_to_close_timeout=datetime.timedelta(seconds=7),
        retry_policy=example_data.retry_policy,
        priority=example_data.priority,
    )
    activity_response = output_models.ActivityOptions.from_proto(await activity_handle)
    assert isinstance(activity_response.retry_policy, temporalio.common.RetryPolicy)
    assert activity_response.retry_policy.maximum_attempts == 3
    assert activity_response.task_queue == TASK_QUEUE
    assert activity_response.schedule_to_close_timeout == datetime.timedelta(seconds=7)
    assert activity_response.priority == example_data.priority
