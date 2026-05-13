from __future__ import annotations

import collections.abc
import datetime
import typing

import temporalio.common
import temporalio.workflow

import output  # pyright: ignore[reportImplicitRelativeImport]

TASK_QUEUE = "demo-task-queue"


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

request: output.SignalWithStartWorkflowExecutionRequest[int, str] = (
    output.SignalWithStartWorkflowExecutionRequest(
        workflow=ExampleWorkflow.run,
        workflow_id="workflow-id",
        task_queue=TASK_QUEUE,
        signal=ExampleWorkflow.wake_up,
        input=(7, "nexus"),
        signal_input=("wake-up",),
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
)

client = output.WorkflowServiceClient()
handle = client.signal_with_start_workflow_execution_args(
    workflow=ExampleWorkflow.run,
    workflow_id="workflow-id",
    task_queue=TASK_QUEUE,
    signal=ExampleWorkflow.wake_up,
    input=(7, "nexus"),
    signal_input=("wake-up",),
)
typed_handle: collections.abc.Awaitable[
    temporalio.workflow.ExternalWorkflowHandle[typing.Any]
] = handle
_ = typed_handle

minimal_handle = client.signal_with_start_workflow_execution_args(
    workflow="ExampleWorkflow",
    workflow_id="workflow-minimal",
    task_queue=TASK_QUEUE,
    signal="wake_up",
)
minimal_typed_handle: collections.abc.Awaitable[
    temporalio.workflow.ExternalWorkflowHandle[typing.Any]
] = minimal_handle
_ = minimal_typed_handle

high_arity_request: output.SignalWithStartWorkflowExecutionRequest[
    *tuple[typing.Any, ...]
] = output.SignalWithStartWorkflowExecutionRequest(
    workflow="ExampleWorkflow",
    workflow_id="workflow-high-arity",
    task_queue=TASK_QUEUE,
    signal=ExampleWorkflow.wake_up_many,
    signal_input=("one", "two", "three", "four", "five", "six", "seven"),
)
_ = typing.assert_type(
    high_arity_request,
    output.SignalWithStartWorkflowExecutionRequest[*tuple[typing.Any, ...]],
)

activity_options = output.ActivityOptions(
    retry_policy=retry_policy,
    task_queue=TASK_QUEUE,
    priority=priority,
)
activity_options.schedule_to_close_timeout = datetime.timedelta(seconds=7)

retry_handle = client.retry_policy_operation(retry_policy)
_ = retry_handle

activity_handle = client.activity_options_operation(activity_options)
_ = activity_handle
