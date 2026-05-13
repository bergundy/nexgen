from __future__ import annotations

from collections.abc import Generator, Sequence
import importlib.util
from pathlib import Path
import sys
import typing

from nexusrpc import Operation
import pytest
import temporalio.api.common.v1.message_pb2 as common_pb2
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.workflow

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT / "output.py"
TASK_QUEUE = "demo-task-queue"


def load_output_module() -> typing.Any:
    spec = importlib.util.spec_from_file_location(
        "generated_start_workflow_output",
        OUTPUT_PATH,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load generated module from {OUTPUT_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


output: typing.Any = load_output_module()


@temporalio.workflow.defn
class ExampleWorkflow:
    @temporalio.workflow.run
    async def run(self, customer_id: str) -> str:
        return customer_id


class FakeOperationHandle:
    def __init__(
        self,
        response: workflowservice_v1.StartWorkflowExecutionResponse,
    ) -> None:
        self._response = response

    def __await__(
        self,
    ) -> Generator[object, None, workflowservice_v1.StartWorkflowExecutionResponse]:
        async def wait_for_result() -> workflowservice_v1.StartWorkflowExecutionResponse:
            return self._response

        return wait_for_result().__await__()


class FakeExternalWorkflowHandle:
    def __init__(self, workflow_id: str, run_id: str | None) -> None:
        self.id = workflow_id
        self.run_id = run_id
        self.cancelled = False
        self.signals: list[tuple[object, tuple[object, ...]]] = []

    async def cancel(self) -> None:
        self.cancelled = True

    async def signal(self, signal: object, *args: object) -> None:
        self.signals.append((signal, args))


class FakePayloadConverter:
    @staticmethod
    def _encode(value: object) -> bytes:
        return f"{type(value).__name__}:{value!r}".encode()

    def to_payloads_wrapper(
        self,
        values: Sequence[object],
    ) -> common_pb2.Payloads:
        payloads = common_pb2.Payloads()
        for value in values:
            payload = payloads.payloads.add()
            payload.data = self._encode(value)
        return payloads


class FakeNexusClient:
    def __init__(self) -> None:
        self.calls: list[tuple[Operation[object, object], object]] = []

    async def start_operation(
        self,
        operation: Operation[object, object],
        input: object,
    ) -> FakeOperationHandle:
        self.calls.append((operation, input))
        assert operation is output.WorkflowService.start_workflow
        assert isinstance(input, workflowservice_v1.StartWorkflowExecutionRequest)
        assert input.namespace == "workflow-namespace"
        assert input.workflow_id == "workflow-id"
        assert input.workflow_type.name == "ExampleWorkflow"
        assert input.task_queue.name == TASK_QUEUE
        assert [payload.data for payload in input.input.payloads] == [b"str:'customer-123'"]

        response = workflowservice_v1.StartWorkflowExecutionResponse()
        response.run_id = "run-123"
        response.started = True
        return FakeOperationHandle(response)


@pytest.fixture
def context() -> tuple[typing.Any, FakeNexusClient, list[FakeExternalWorkflowHandle]]:
    fake_client = FakeNexusClient()
    fake_payload_converter = FakePayloadConverter()
    created_external_handles: list[FakeExternalWorkflowHandle] = []

    def fake_create_nexus_client(*, service: type[object], endpoint: str) -> FakeNexusClient:
        assert service is output.WorkflowService
        assert endpoint == "__temporal_system"
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

    workflow_module = output.workflow  # pyright: ignore[reportPrivateLocalImportUsage]
    setattr(workflow_module, "create_nexus_client", fake_create_nexus_client)
    setattr(workflow_module, "payload_converter", fake_workflow_payload_converter)
    setattr(workflow_module, "info", fake_workflow_info)
    setattr(workflow_module, "get_external_workflow_handle", fake_get_external_workflow_handle)

    client = output.WorkflowServiceClient()
    return client, fake_client, created_external_handles


def test_generated_metadata() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated file at {OUTPUT_PATH}"
    start_operation = output.WorkflowService.start_workflow
    cancel_operation = output.WorkflowService.cancel_workflow
    registry = output.__nexus_operation_registry__

    assert isinstance(start_operation, Operation)
    assert start_operation.name == "StartWorkflow"
    assert registry[("WorkflowService", "StartWorkflow")] is start_operation
    assert isinstance(cancel_operation, Operation)
    assert cancel_operation.name == "CancelWorkflow"
    assert registry[("WorkflowService", "CancelWorkflow")] is cancel_operation


def test_cancel_workflow_request_serializes(
    context: tuple[typing.Any, FakeNexusClient, list[FakeExternalWorkflowHandle]],
) -> None:
    _client, _fake_client, _created_external_handles = context

    request = output.RequestCancelWorkflowExecutionRequest(
        workflow_execution=output.WorkflowExecution(workflow_id="workflow-id"),
        reason="user requested cancellation",
    )
    proto = request.to_proto()

    assert proto.namespace == "workflow-namespace"
    assert proto.workflow_execution.workflow_id == "workflow-id"
    assert proto.workflow_execution.run_id == ""
    assert proto.reason == "user requested cancellation"


async def test_start_workflow_returns_wrapper_handle(
    context: tuple[typing.Any, FakeNexusClient, list[FakeExternalWorkflowHandle]],
) -> None:
    client, fake_client, created_external_handles = context

    handle = await client.start_workflow_args(
        workflow=ExampleWorkflow.run,
        input=("customer-123",),
        workflow_id="workflow-id",
        task_queue=TASK_QUEUE,
    )

    assert len(fake_client.calls) == 1
    assert isinstance(handle, output.StartedWorkflowHandle)
    assert handle.workflow_id == "workflow-id"
    assert handle.run_id == "run-123"
    assert len(created_external_handles) == 1

    await handle.cancel()
    assert created_external_handles[0].cancelled is True

    with pytest.raises(NotImplementedError):
        await handle.get_result()
