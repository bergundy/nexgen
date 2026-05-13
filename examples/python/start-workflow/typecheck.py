from __future__ import annotations

import collections.abc
import typing

import temporalio.workflow

import output  # pyright: ignore[reportImplicitRelativeImport]

TASK_QUEUE = "demo-task-queue"


@temporalio.workflow.defn
class ExampleWorkflow:
    @temporalio.workflow.run
    async def run(self, customer_id: str) -> str:
        return customer_id


request: output.StartWorkflowExecutionRequest[str] = output.StartWorkflowExecutionRequest(
    workflow=ExampleWorkflow.run,
    input=("customer-123",),
    workflow_id="workflow-id",
    task_queue=TASK_QUEUE,
)

client = output.WorkflowServiceClient()
handle_task = client.start_workflow_args(
    workflow=ExampleWorkflow.run,
    input=("customer-123",),
    workflow_id="workflow-id",
    task_queue=TASK_QUEUE,
)
typed_handle_task: collections.abc.Awaitable[
    output.StartedWorkflow
] = handle_task
_ = typed_handle_task

named_handle_task = client.start_workflow_args(
    workflow="ExampleWorkflow",
    workflow_id="workflow-id-named",
    task_queue=TASK_QUEUE,
)
typed_named_handle_task: collections.abc.Awaitable[
    output.StartedWorkflow
] = named_handle_task
_ = typed_named_handle_task

cancel_task = client.cancel_workflow_args(
    workflow_execution=output.WorkflowExecution(workflow_id="workflow-id"),
)
_ = cancel_task


async def use_handle() -> None:
    handle = await handle_task
    await handle.cancel()
    result_task: collections.abc.Awaitable[collections.abc.Sequence[typing.Any]] = (
        handle.get_result()
    )
    _ = result_task
