from __future__ import annotations

import asyncio
from collections.abc import Generator
from datetime import timedelta
from pathlib import Path
from typing import final, override

from nexusrpc import Operation
import temporalio.api.common.v1 as common_v1
import temporalio.api.taskqueue.v1 as taskqueue_v1
import temporalio.api.workflowservice.v1 as workflowservice_v1
import temporalio.common as temporal_common
from temporalio import workflow

import output

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT / "output.py"


@workflow.defn
class ExampleWorkflow:
    @workflow.run
    async def run(self) -> None:
        raise RuntimeError("validation workflow should never execute")

    @workflow.signal
    def wake_up(self) -> None:
        pass


@final
class FakeOperationHandle(
    workflow.NexusOperationHandle[
        workflowservice_v1.SignalWithStartWorkflowExecutionResponse
    ]
):
    _response: workflowservice_v1.SignalWithStartWorkflowExecutionResponse

    def __init__(
        self,
        response: workflowservice_v1.SignalWithStartWorkflowExecutionResponse,
    ) -> None:
        self._response = response

    @override
    def cancel(self) -> bool:
        return False

    @override
    def __await__(
        self,
    ) -> Generator[
        object,
        None,
        workflowservice_v1.SignalWithStartWorkflowExecutionResponse,
    ]:
        async def wait_for_result() -> (
            workflowservice_v1.SignalWithStartWorkflowExecutionResponse
        ):
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
        assert isinstance(
            input,
            workflowservice_v1.SignalWithStartWorkflowExecutionRequest,
        )
        assert input.namespace == "default"
        assert input.workflow_id == "workflow-123"
        assert input.signal_name == "wake_up"
        assert input.workflow_type.name == "ExampleWorkflow"
        assert input.task_queue.name == "default"

        response = workflowservice_v1.SignalWithStartWorkflowExecutionResponse()
        response.run_id = "run-123"
        response.started = True
        return FakeOperationHandle(response)


async def main() -> None:
    created_clients: list[tuple[str, type[object]]] = []
    created_handles: list[FakeExternalWorkflowHandle] = []

    operation = output.WorkflowService.signal_with_start_workflow_execution
    registry = output.__nexus_operation_registry__
    operation_key = (
        "WorkflowService",
        "SignalWithStartWorkflowExecution",
    )

    assert isinstance(operation, Operation)
    assert (
        operation.name == "SignalWithStartWorkflowExecution"
    ), "unexpected operation name"
    assert registry[operation_key] is operation, "registry entry should point at the service op"
    assert hasattr(
        workflowservice_v1,
        "SignalWithStartWorkflowExecutionRequest",
    ), "request proto missing from installed temporalio package"
    assert hasattr(
        workflowservice_v1,
        "SignalWithStartWorkflowExecutionResponse",
    ), "response proto missing from installed temporalio package"

    retry_policy = temporal_common.RetryPolicy(
        initial_interval=timedelta(seconds=2),
        maximum_interval=timedelta(seconds=10),
        maximum_attempts=3,
    )
    retry_policy_proto = common_v1.RetryPolicy()
    retry_policy.apply_to_proto(retry_policy_proto)

    request = output.SignalWithStartWorkflowExecutionRequestModel(
        namespace="default",
        workflow_id="workflow-123",
        workflow_type=common_v1.WorkflowType(name="ExampleWorkflow"),
        task_queue=taskqueue_v1.TaskQueue(name="default"),
        signal_name="wake_up",
        retry_policy=retry_policy_proto,
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
    assert request_proto.signal_name == "wake_up"
    assert request_proto.retry_policy.maximum_attempts == 3
    assert request_proto.retry_policy.initial_interval.seconds == 2
    assert request_proto.retry_policy.maximum_interval.seconds == 10

    client = FakeNexusClient()
    original_create_nexus_client = workflow.create_nexus_client
    original_get_external_workflow_handle = workflow.get_external_workflow_handle

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

    workflow.create_nexus_client = fake_create_nexus_client
    workflow.get_external_workflow_handle = fake_get_external_workflow_handle

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
            "workflow-123",
            signal=ExampleWorkflow.wake_up,
            workflow=ExampleWorkflow.run,
            retry_policy=retry_policy,
        )
        defaulted_handle = await workflow_client.signal_with_start_workflow(
            "workflow-123",
            signal=ExampleWorkflow.wake_up,
            workflow=ExampleWorkflow.run,
        )
    finally:
        workflow.create_nexus_client = original_create_nexus_client
        workflow.get_external_workflow_handle = original_get_external_workflow_handle

    assert created_clients == [("__temporal_system", output.WorkflowService)]
    assert len(client.calls) == 3
    assert client.calls[0][0] is operation
    assert client.calls[1][0] is operation
    assert client.calls[2][0] is operation
    first_input = client.calls[0][1]
    second_input = client.calls[1][1]
    third_input = client.calls[2][1]
    assert isinstance(first_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert isinstance(second_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert isinstance(third_input, workflowservice_v1.SignalWithStartWorkflowExecutionRequest)
    assert first_input.retry_policy.maximum_attempts == 3
    assert first_input.retry_policy.initial_interval.seconds == 2
    assert first_input.retry_policy.maximum_interval.seconds == 10
    assert second_input.retry_policy.maximum_attempts == 3
    assert second_input.retry_policy.initial_interval.seconds == 2
    assert second_input.retry_policy.maximum_interval.seconds == 10
    assert not third_input.HasField("retry_policy")
    assert response.run_id == "run-123"
    assert response.started is True
    assert len(created_handles) == 2
    assert ergonomic_handle is created_handles[0]
    assert defaulted_handle is created_handles[1]
    assert ergonomic_handle.id == "workflow-123"
    assert ergonomic_handle.run_id == "run-123"
    assert defaulted_handle.id == "workflow-123"
    assert defaulted_handle.run_id == "run-123"

    print(f"Validated generated module: {OUTPUT_PATH}")
    print(f"Operation registry contains {len(registry)} entry")
    print("Dataclass conversion, ergonomic API conversion, defaulted parameters, and start_operation wrapper look correct")


if __name__ == "__main__":
    asyncio.run(main())
