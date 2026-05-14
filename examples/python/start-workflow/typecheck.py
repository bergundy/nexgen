# pyright: reportUnusedVariable=false
# ruff: noqa: F841

from __future__ import annotations

import collections.abc
import typing

import temporalio.api.workflowservice.v1.request_response_pb2
import temporalio.workflow

import start_workflow as output  # pyright: ignore[reportImplicitRelativeImport]
import start_workflow.models as output_models  # pyright: ignore[reportImplicitRelativeImport]

TASK_QUEUE = "demo-task-queue"


@temporalio.workflow.defn
class ExampleWorkflow:
    @temporalio.workflow.run
    async def run(self, customer_id: str) -> str:
        return customer_id


handle_task = output.start_workflow(
    workflow=ExampleWorkflow.run,
    input="customer-123",
    workflow_id="workflow-id",
    task_queue=TASK_QUEUE,
)
typed_handle_task: collections.abc.Awaitable[output.StartedWorkflow] = handle_task

named_handle_task: collections.abc.Awaitable[output.StartedWorkflow] = (
    output.start_workflow(
        workflow="ExampleWorkflow",
        workflow_id="workflow-id-named",
        task_queue=TASK_QUEUE,
    )
)

cancel_task: collections.abc.Awaitable[
    temporalio.workflow.NexusOperationHandle[
        temporalio.api.workflowservice.v1.request_response_pb2.RequestCancelWorkflowExecutionResponse
    ]
] = output.cancel_workflow(
    workflow_execution=output_models.WorkflowExecution(workflow_id="workflow-id"),
)


async def use_handle() -> None:
    handle = await handle_task
    await handle.cancel()
    restarted_handle_task: collections.abc.Awaitable[output.StartedWorkflow] = (
        handle.restart_workflow(
            workflow=ExampleWorkflow.run,
            task_queue=TASK_QUEUE,
        )
    )
    result_task: collections.abc.Awaitable[collections.abc.Sequence[typing.Any]] = (
        handle.get_result()
    )
