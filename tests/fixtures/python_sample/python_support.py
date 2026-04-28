import temporalio.api.common.v1
import temporalio.api.taskqueue.v1
import temporalio.common


class _WorkflowDefinitionLike(typing.Protocol):
    name: str | None


class _SignalDefinitionLike(typing.Protocol):
    name: str | None


def _workflow_name(
    name_or_run_fn: str | collections.abc.Callable[..., collections.abc.Awaitable[object]],
) -> str:
    if isinstance(name_or_run_fn, str):
        return name_or_run_fn
    definition = typing.cast(
        _WorkflowDefinitionLike | None,
        getattr(name_or_run_fn, "__temporal_workflow_definition", None),
    )
    if definition is None:
        raise ValueError(
            "Workflow callable is missing Temporal workflow metadata; use a @workflow.run method from a @workflow.defn class."
        )
    if not definition.name:
        raise ValueError("Cannot use a dynamic workflow with SignalWithStartWorkflow")
    return definition.name


def _signal_name(name_or_fn: str | collections.abc.Callable[..., object]) -> str:
    if isinstance(name_or_fn, str):
        return name_or_fn
    definition = typing.cast(
        _SignalDefinitionLike | None,
        getattr(name_or_fn, "__temporal_signal_definition", None),
    )
    if definition is None:
        raise RuntimeError(
            "Signal callable is missing Temporal signal metadata; use a @workflow.signal method."
        )
    if not definition.name:
        raise RuntimeError("Cannot invoke a dynamic signal definition")
    return definition.name


def build_signal_with_start_workflow_request(
    *,
    workflow_id: str,
    signal: str | collections.abc.Callable[..., object],
    workflow: str | collections.abc.Callable[..., collections.abc.Awaitable[object]],
    retry_policy: temporalio.common.RetryPolicy | None,
) -> SignalWithStartWorkflowExecutionRequestModel:
    return SignalWithStartWorkflowExecutionRequestModel(
        namespace="default",
        workflow_id=workflow_id,
        workflow_type=temporalio.api.common.v1.WorkflowType(
            name=_workflow_name(workflow),
        ),
        task_queue=temporalio.api.taskqueue.v1.TaskQueue(name="default"),
        signal_name=_signal_name(signal),
        retry_policy=_retry_policy_to_proto(retry_policy),
    )


def _retry_policy_to_proto(
    retry_policy: temporalio.common.RetryPolicy | None,
) -> temporalio.api.common.v1.RetryPolicy | None:
    if retry_policy is None:
        return None

    proto = temporalio.api.common.v1.RetryPolicy()
    retry_policy.apply_to_proto(proto)
    return proto


def signal_with_start_workflow_response_to_external_workflow_handle(
    *,
    request: SignalWithStartWorkflowExecutionRequestModel,
    response: SignalWithStartWorkflowExecutionResponseModel,
) -> workflow.ExternalWorkflowHandle[object]:
    if request.workflow_id is None:
        raise ValueError("workflow_id must be set to build an ExternalWorkflowHandle")
    return typing.cast(
        workflow.ExternalWorkflowHandle[object],
        workflow.get_external_workflow_handle(
            request.workflow_id,
            run_id=response.run_id,
        ),
    )
