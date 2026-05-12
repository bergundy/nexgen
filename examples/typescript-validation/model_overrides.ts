export function retryPolicyFromProto(
  proto: temporal.api.common.v1.IRetryPolicy,
): common.RetryPolicy {
  return common.decompileRetryPolicy(proto) ?? {};
}

export function retryPolicyToProto(
  retryPolicy: common.RetryPolicy,
): temporal.api.common.v1.IRetryPolicy {
  return common.compileRetryPolicy(retryPolicy);
}
