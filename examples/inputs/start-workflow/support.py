import collections.abc
import typing

import temporalio.workflow

_ResultT = typing.TypeVar("_ResultT")


class StartedWorkflowHandle(typing.Generic[_ResultT]):
    def __init__(
        self,
        handle: temporalio.workflow.ExternalWorkflowHandle[_ResultT],
    ) -> None:
        self._handle: temporalio.workflow.ExternalWorkflowHandle[_ResultT] = handle

    @property
    def workflow_id(self) -> str:
        return self._handle.id

    @property
    def run_id(self) -> str | None:
        return self._handle.run_id

    async def cancel(self) -> None:
        await self._handle.cancel()

    async def signal(
        self,
        signal: str | collections.abc.Callable[..., typing.Any],
        *args: typing.Any,
    ) -> None:
        await self._handle.signal(signal, *args)

    async def get_result(self) -> _ResultT:
        raise NotImplementedError(
            "result retrieval is not yet implemented for started workflow handles"
        )


def started_workflow_handle_from_proto(
    workflow_id: str,
    run_id: str | None,
) -> StartedWorkflowHandle[typing.Any]:
    handle = temporalio.workflow.get_external_workflow_handle(
        workflow_id,
        run_id=run_id,
    )
    return StartedWorkflowHandle(handle)
