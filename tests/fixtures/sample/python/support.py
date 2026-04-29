from datetime import timedelta

import temporalio.api.deployment.v1
import temporalio.api.enums.v1
import temporalio.api.sdk.v1
import temporalio.api.workflow.v1
import temporalio.common
import temporalio.converter
import temporalio.workflow as temporalio_workflow


class _WorkflowDefinitionLike(typing.Protocol):
    name: str | None


class _SignalDefinitionLike(typing.Protocol):
    name: str | None


def _workflow_name(
    name_or_run_fn: str
    | collections.abc.Callable[..., collections.abc.Awaitable[typing.Any]],
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
        raise ValueError("Cannot invoke a dynamic workflow explicitly")
    return definition.name


def _signal_name(name_or_fn: str | collections.abc.Callable[..., typing.Any]) -> str:
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
    workflow: str
    | collections.abc.Callable[..., collections.abc.Awaitable[typing.Any]],
    signal: str | collections.abc.Callable[..., typing.Any],
    id: str,
    signal_args: collections.abc.Sequence[typing.Any],
    workflow_args: collections.abc.Sequence[typing.Any],
    task_queue: str | None,
    execution_timeout: timedelta | None,
    run_timeout: timedelta | None,
    task_timeout: timedelta | None,
    id_reuse_policy: temporalio.common.WorkflowIDReusePolicy,
    id_conflict_policy: temporalio.common.WorkflowIDConflictPolicy,
    retry_policy: temporalio.common.RetryPolicy | None,
    cron_schedule: str,
    memo: collections.abc.Mapping[str, typing.Any] | None,
    search_attributes: temporalio.common.TypedSearchAttributes | None,
    static_summary: str | None,
    static_details: str | None,
    start_delay: timedelta | None,
    request_id: str | None,
    priority: temporalio.common.Priority,
    versioning_override: temporalio.common.VersioningOverride | None,
) -> SignalWithStartWorkflowExecutionRequestModel:
    payload_converter = temporalio_workflow.payload_converter()
    workflow_info = temporalio_workflow.info()
    request = temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest(
        signal_name=_signal_name(signal)
    )
    if signal_args:
        request.signal_input.payloads.extend(payload_converter.to_payloads(signal_args))

    request.namespace = workflow_info.namespace
    request.workflow_type.name = _workflow_name(workflow)
    request.workflow_id = id
    request.task_queue.name = (
        task_queue if task_queue is not None else workflow_info.task_queue
    )
    if workflow_args:
        request.input.payloads.extend(payload_converter.to_payloads(workflow_args))
    if execution_timeout is not None:
        request.workflow_execution_timeout.FromTimedelta(execution_timeout)
    if run_timeout is not None:
        request.workflow_run_timeout.FromTimedelta(run_timeout)
    if task_timeout is not None:
        request.workflow_task_timeout.FromTimedelta(task_timeout)
    if request_id is not None:
        request.request_id = request_id
    request.workflow_id_reuse_policy = typing.cast(
        temporalio.api.enums.v1.WorkflowIdReusePolicy.ValueType,
        int(id_reuse_policy),
    )
    request.workflow_id_conflict_policy = typing.cast(
        temporalio.api.enums.v1.WorkflowIdConflictPolicy.ValueType,
        int(id_conflict_policy),
    )
    if retry_policy is not None:
        retry_policy.apply_to_proto(request.retry_policy)
    request.cron_schedule = cron_schedule
    if memo is not None:
        memo_values = typing.cast(collections.abc.Mapping[str, object], memo)
        for key, value in memo_values.items():
            request.memo.fields[key].CopyFrom(
                payload_converter.to_payload(value)
            )
    if search_attributes is not None:
        temporalio.converter.encode_search_attributes(
            search_attributes, request.search_attributes
        )
    user_metadata = _build_user_metadata(
        payload_converter, static_summary, static_details
    )
    if user_metadata is not None:
        request.user_metadata.CopyFrom(user_metadata)
    if start_delay is not None:
        request.workflow_start_delay.FromTimedelta(start_delay)
    request.priority.CopyFrom(_priority_to_proto(priority))
    if versioning_override is not None:
        request.versioning_override.CopyFrom(
            _versioning_override_to_proto(versioning_override)
        )

    return SignalWithStartWorkflowExecutionRequestModel.from_proto(request)


def sdk_retry_policy_to_model(
    retry_policy: temporalio.common.RetryPolicy | None,
) -> RetryPolicyModel | None:
    if retry_policy is None:
        return None

    proto = temporalio.api.common.v1.RetryPolicy()
    retry_policy.apply_to_proto(proto)
    return RetryPolicyModel.from_proto(proto)


def sdk_task_queue_to_model(task_queue: str | None) -> TaskQueueModel | None:
    if task_queue is None:
        return None
    return TaskQueueModel(name=task_queue)


def task_queue_model_to_sdk(task_queue: TaskQueueModel | None) -> str | None:
    if task_queue is None:
        return None
    return task_queue.name


def timedelta_to_model(duration: timedelta | None) -> DurationModel | None:
    if duration is None:
        return None
    proto = google.protobuf.duration_pb2.Duration()
    proto.FromTimedelta(duration)
    return DurationModel.from_proto(proto)


def duration_model_to_timedelta(
    duration: DurationModel | None,
) -> timedelta | None:
    if duration is None:
        return None
    return duration.to_proto().ToTimedelta()


def sdk_priority_to_model(
    priority: temporalio.common.Priority,
) -> PriorityModel:
    return PriorityModel.from_proto(_priority_to_proto(priority))


def priority_model_to_sdk(
    priority: PriorityModel | None,
) -> temporalio.common.Priority:
    if priority is None:
        return temporalio.common.Priority.default
    return temporalio.common.Priority(
        priority_key=priority.priority_key,
        fairness_key=priority.fairness_key,
        fairness_weight=priority.fairness_weight,
    )


def _build_user_metadata(
    payload_converter: temporalio.converter.PayloadConverter,
    static_summary: str | None,
    static_details: str | None,
) -> temporalio.api.sdk.v1.UserMetadata | None:
    if static_summary is None and static_details is None:
        return None

    metadata = temporalio.api.sdk.v1.UserMetadata()
    if static_summary is not None:
        metadata.summary.CopyFrom(payload_converter.to_payload(static_summary))
    if static_details is not None:
        metadata.details.CopyFrom(payload_converter.to_payload(static_details))
    return metadata


def _priority_to_proto(
    priority: temporalio.common.Priority,
) -> temporalio.api.common.v1.Priority:
    proto = temporalio.api.common.v1.Priority()
    if priority.priority_key is not None:
        proto.priority_key = priority.priority_key
    if priority.fairness_key is not None:
        proto.fairness_key = priority.fairness_key
    if priority.fairness_weight is not None:
        proto.fairness_weight = priority.fairness_weight
    return proto


def _versioning_override_to_proto(
    versioning_override: temporalio.common.VersioningOverride,
) -> temporalio.api.workflow.v1.VersioningOverride:
    if isinstance(versioning_override, temporalio.common.AutoUpgradeVersioningOverride):
        return temporalio.api.workflow.v1.VersioningOverride(
            behavior=temporalio.api.enums.v1.VersioningBehavior.VERSIONING_BEHAVIOR_AUTO_UPGRADE,
            auto_upgrade=True,
        )
    if isinstance(versioning_override, temporalio.common.PinnedVersioningOverride):
        return temporalio.api.workflow.v1.VersioningOverride(
            behavior=temporalio.api.enums.v1.VersioningBehavior.VERSIONING_BEHAVIOR_PINNED,
            pinned_version=versioning_override.version.to_canonical_string(),
            pinned=temporalio.api.workflow.v1.VersioningOverride.PinnedOverride(
                version=temporalio.api.deployment.v1.WorkerDeploymentVersion(
                    build_id=versioning_override.version.build_id,
                    deployment_name=versioning_override.version.deployment_name,
                ),
                behavior=temporalio.api.workflow.v1.VersioningOverride.PinnedOverrideBehavior.PINNED_OVERRIDE_BEHAVIOR_PINNED,
            ),
        )
    raise TypeError(
        f"Unsupported versioning override type: {type(versioning_override).__name__}"
    )


def signal_with_start_workflow_response_to_external_workflow_handle(
    *,
    request: SignalWithStartWorkflowExecutionRequestModel,
    response: SignalWithStartWorkflowExecutionResponseModel,
) -> workflow.ExternalWorkflowHandle[typing.Any]:
    if request.workflow_id is None:
        raise ValueError("workflow_id must be set to build an ExternalWorkflowHandle")
    return temporalio_workflow.get_external_workflow_handle(
        request.workflow_id,
        run_id=response.run_id,
    )


def retry_policy_model_to_sdk(
    retry_policy: RetryPolicyModel | None,
) -> temporalio.common.RetryPolicy | None:
    if retry_policy is None or retry_policy.initial_interval is None:
        return None
    return temporalio.common.RetryPolicy.from_proto(retry_policy.to_proto())


def activity_options_model_to_config(
    *,
    request: ActivityOptionsModel,
    response: ActivityOptionsModel,
) -> workflow.ActivityConfig:
    task_queue = response.task_queue if response.task_queue is not None else request.task_queue
    schedule_to_close_timeout = (
        response.schedule_to_close_timeout
        if response.schedule_to_close_timeout is not None
        else request.schedule_to_close_timeout
    )
    schedule_to_start_timeout = (
        response.schedule_to_start_timeout
        if response.schedule_to_start_timeout is not None
        else request.schedule_to_start_timeout
    )
    start_to_close_timeout = (
        response.start_to_close_timeout
        if response.start_to_close_timeout is not None
        else request.start_to_close_timeout
    )
    heartbeat_timeout = (
        response.heartbeat_timeout
        if response.heartbeat_timeout is not None
        else request.heartbeat_timeout
    )
    retry_policy = (
        response.retry_policy if response.retry_policy is not None else request.retry_policy
    )
    priority = response.priority if response.priority is not None else request.priority
    config: workflow.ActivityConfig = {
        "task_queue": task_queue_model_to_sdk(task_queue),
        "schedule_to_close_timeout": duration_model_to_timedelta(
            schedule_to_close_timeout
        ),
        "schedule_to_start_timeout": duration_model_to_timedelta(
            schedule_to_start_timeout
        ),
        "start_to_close_timeout": duration_model_to_timedelta(start_to_close_timeout),
        "heartbeat_timeout": duration_model_to_timedelta(heartbeat_timeout),
        "retry_policy": retry_policy_model_to_sdk(retry_policy),
        "priority": priority_model_to_sdk(priority),
    }
    return config
