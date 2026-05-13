export class StartedWorkflowHandle {
  public readonly workflowId: string;
  public readonly runId: string | undefined;

  public constructor(
    private readonly handle: workflow.ExternalWorkflowHandle,
  ) {
    this.workflowId = handle.workflowId;
    this.runId = handle.runId;
  }

  public async cancel(): Promise<void> {
    await this.handle.cancel();
  }

  public async signal<Args extends any[] = [], Name extends string = string>(
    def: workflow.SignalDefinition<Args, Name> | string,
    ...args: Args
  ): Promise<void> {
    await this.handle.signal(def, ...args);
  }

  public async getResult(): Promise<unknown> {
    throw new Error(
      "result retrieval is not yet implemented for started workflow handles",
    );
  }
}

export function startedWorkflowHandleFromProto(
  workflowId: string,
  runId: string | undefined,
): StartedWorkflowHandle {
  return new StartedWorkflowHandle(
    workflow.getExternalWorkflowHandle(workflowId, runId),
  );
}
