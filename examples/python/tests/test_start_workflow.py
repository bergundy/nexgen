from __future__ import annotations

from collections.abc import Generator, Sequence
from pathlib import Path

from nexusrpc import Operation
import pytest
import temporalio.api.common.v1.message_pb2 as common_pb2
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.workflow

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT.parent / "start_workflow"
TASK_QUEUE = "demo-task-queue"

import start_workflow as output
import start_workflow.models as output_models

START_WORKFLOW_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "StartWorkflow")
]
RESTART_WORKFLOW_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "RestartWorkflow")
]
CANCEL_WORKFLOW_OPERATION = output.__nexus_operation_registry__[
    ("WorkflowService", "CancelWorkflow")
]


@temporalio.workflow.defn
class ExampleWorkflow:
    @temporalio.workflow.run
    async def run(self, customer_id: str) -> str:
        return customer_id


class FakeOperationHandle:
    def __init__(
        self,
        response: object,
    ) -> None:
        self._response: object = response

    def __await__(
        self,
    ) -> Generator[object, None, object]:
        async def wait_for_result() -> object:
            return self._response

        return wait_for_result().__await__()


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
        if operation is START_WORKFLOW_OPERATION:
            assert isinstance(input, workflowservice_v1.StartWorkflowExecutionRequest)
            assert input.namespace == "workflow-namespace"
            assert input.workflow_id == "workflow-id"
            assert input.workflow_type.name == "ExampleWorkflow"
            assert input.task_queue.name == TASK_QUEUE
            assert [payload.data for payload in input.input.payloads] == [
                b"str:'customer-123'"
            ]

            response = workflowservice_v1.StartWorkflowExecutionResponse()
            response.run_id = "run-123"
            response.started = True
            return FakeOperationHandle(response)

        if operation is RESTART_WORKFLOW_OPERATION:
            assert isinstance(input, workflowservice_v1.StartWorkflowExecutionRequest)
            assert input.namespace == "workflow-namespace"
            assert input.workflow_id == "workflow-id"
            assert input.workflow_type.name == "ExampleWorkflow"
            assert input.task_queue.name == TASK_QUEUE
            assert not input.HasField("input")

            response = workflowservice_v1.StartWorkflowExecutionResponse()
            response.run_id = "run-456"
            response.started = True
            return FakeOperationHandle(response)

        assert operation is CANCEL_WORKFLOW_OPERATION
        assert isinstance(
            input, workflowservice_v1.RequestCancelWorkflowExecutionRequest
        )
        assert input.namespace == "workflow-namespace"
        assert input.workflow_execution.workflow_id == "workflow-id"
        assert input.workflow_execution.run_id == "run-123"
        response = workflowservice_v1.RequestCancelWorkflowExecutionResponse()
        return FakeOperationHandle(response)


@pytest.fixture
def fake_client() -> FakeNexusClient:
    fake_client = FakeNexusClient()
    fake_payload_converter = FakePayloadConverter()

    def fake_create_nexus_client(
        *, service: type[object], endpoint: str
    ) -> FakeNexusClient:
        assert service.__name__ == "WorkflowService"
        assert endpoint == "__temporal_system"
        return fake_client

    def fake_workflow_payload_converter() -> FakePayloadConverter:
        return fake_payload_converter

    class FakeWorkflowInfo:
        namespace: str = "workflow-namespace"

    def fake_workflow_info() -> FakeWorkflowInfo:
        return FakeWorkflowInfo()

    workflow_module = temporalio.workflow
    setattr(workflow_module, "create_nexus_client", fake_create_nexus_client)
    setattr(workflow_module, "payload_converter", fake_workflow_payload_converter)
    setattr(workflow_module, "info", fake_workflow_info)

    return fake_client


def test_generated_metadata() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated package at {OUTPUT_PATH}"
    start_operation = START_WORKFLOW_OPERATION
    restart_operation = RESTART_WORKFLOW_OPERATION
    cancel_operation = CANCEL_WORKFLOW_OPERATION
    registry = output.__nexus_operation_registry__

    assert isinstance(start_operation, Operation)
    assert start_operation.name == "StartWorkflow"
    assert registry[("WorkflowService", "StartWorkflow")] is start_operation
    assert isinstance(cancel_operation, Operation)
    assert cancel_operation.name == "CancelWorkflow"
    assert registry[("WorkflowService", "CancelWorkflow")] is cancel_operation
    assert isinstance(restart_operation, Operation)
    assert restart_operation.name == "RestartWorkflow"
    assert registry[("WorkflowService", "RestartWorkflow")] is restart_operation
    assert not hasattr(output, "WorkflowService")
    assert not hasattr(output, "StartWorkflowExecutionRequest")
    assert hasattr(output, "StartedWorkflow")


def test_cancel_workflow_request_serializes(
    fake_client: FakeNexusClient,
) -> None:
    assert fake_client.calls == []

    request = output_models.RequestCancelWorkflowExecutionRequest(
        workflow_execution=output_models.WorkflowExecution(workflow_id="workflow-id"),
        reason="user requested cancellation",
    )
    proto = request.to_proto()

    assert proto.namespace == "workflow-namespace"
    assert proto.workflow_execution.workflow_id == "workflow-id"
    assert proto.workflow_execution.run_id == ""
    assert proto.reason == "user requested cancellation"


async def test_start_workflow_returns_wrapper_handle(
    fake_client: FakeNexusClient,
) -> None:
    handle = await output.start_workflow(
        workflow=ExampleWorkflow.run,
        input="customer-123",
        workflow_id="workflow-id",
        task_queue=TASK_QUEUE,
    )

    assert len(fake_client.calls) == 1
    assert isinstance(handle, output.StartedWorkflow)
    assert handle.namespace == "workflow-namespace"
    assert handle.workflow_id == "workflow-id"
    assert handle.run_id == "run-123"

    await handle.cancel()
    assert len(fake_client.calls) == 2
    cancel_operation, cancel_request = fake_client.calls[1]
    assert cancel_operation is CANCEL_WORKFLOW_OPERATION
    assert isinstance(
        cancel_request,
        workflowservice_v1.RequestCancelWorkflowExecutionRequest,
    )
    assert cancel_request.namespace == "workflow-namespace"
    assert cancel_request.workflow_execution.workflow_id == "workflow-id"
    assert cancel_request.workflow_execution.run_id == "run-123"

    restarted_handle = await handle.restart_workflow(
        workflow=ExampleWorkflow.run,
        task_queue=TASK_QUEUE,
    )
    assert len(fake_client.calls) == 3
    assert isinstance(restarted_handle, output.StartedWorkflow)
    assert restarted_handle.namespace == "workflow-namespace"
    assert restarted_handle.workflow_id == "workflow-id"
    assert restarted_handle.run_id == "run-456"

    restart_operation, restart_request = fake_client.calls[2]
    assert restart_operation is RESTART_WORKFLOW_OPERATION
    assert isinstance(
        restart_request,
        workflowservice_v1.StartWorkflowExecutionRequest,
    )
    assert restart_request.namespace == "workflow-namespace"
    assert restart_request.workflow_id == "workflow-id"
    assert restart_request.workflow_type.name == "ExampleWorkflow"
    assert restart_request.task_queue.name == TASK_QUEUE
    assert not restart_request.HasField("input")

    result_error: pytest.ExceptionInfo[NotImplementedError]
    with pytest.raises(NotImplementedError) as result_error:
        _ = await restarted_handle.get_result()
    assert (
        result_error.value.args[0]
        == "started-workflow.get_result is not yet implemented"
    )
