from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

from nexusrpc import Operation
import pytest
import temporalio.workflow

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT.parent / "user_service"

import user_service as output
import user_service.models as output_models

GET_USER_OPERATION = output.__nexus_operation_registry__[("UserService", "GetUser")]
UPDATE_EMAIL_OPERATION = output.__nexus_operation_registry__[
    ("UserService", "UpdateEmail")
]


def user_resource(
    *,
    email: str,
) -> output.User:
    return output.User(
        user_id="user-123",
        email=email,
    )


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


class FakeNexusClient:
    def __init__(self) -> None:
        self.calls: list[tuple[str, object]] = []

    async def start_operation(
        self,
        operation: str,
        input: object,
        *,
        output_type: type[object] | None = None,
    ) -> FakeOperationHandle:
        self.calls.append((operation, input))
        if operation == "GetUser":
            assert output_type is output.User
            assert isinstance(input, output_models.GetUserRequest)
            assert input.user_id == "user-123"
            return FakeOperationHandle(user_resource(email="old@example.com"))

        assert operation == "UpdateEmail"
        assert output_type is output.User
        assert isinstance(input, output_models.UpdateEmailRequest)
        assert input.user_id == "user-123"
        assert input.email == "new@example.com"
        return FakeOperationHandle(user_resource(email=input.email))


@pytest.fixture
def fake_client(monkeypatch: pytest.MonkeyPatch) -> FakeNexusClient:
    fake_client = FakeNexusClient()

    def fake_create_nexus_client(*, service: str, endpoint: str) -> FakeNexusClient:
        assert service == "UserService"
        assert endpoint == "__user_service"
        return fake_client

    monkeypatch.setattr(
        temporalio.workflow,
        "create_nexus_client",
        fake_create_nexus_client,
    )
    return fake_client


def test_generated_metadata() -> None:
    assert OUTPUT_PATH.exists(), f"expected generated package at {OUTPUT_PATH}"
    registry = output.__nexus_operation_registry__

    assert isinstance(GET_USER_OPERATION, Operation)
    assert GET_USER_OPERATION.name == "GetUser"
    assert registry[("UserService", "GetUser")] is GET_USER_OPERATION
    assert isinstance(UPDATE_EMAIL_OPERATION, Operation)
    assert UPDATE_EMAIL_OPERATION.name == "UpdateEmail"
    assert registry[("UserService", "UpdateEmail")] is UPDATE_EMAIL_OPERATION
    assert not hasattr(output, "UserService")
    assert hasattr(output, "User")
    assert not hasattr(output_models.GetUserRequest, "to_proto")


async def test_get_user_returns_user_resource(
    fake_client: FakeNexusClient,
) -> None:
    user = await output.get_user(user_id="user-123")

    assert len(fake_client.calls) == 1
    assert isinstance(user, output.User)
    assert user.user_id == "user-123"
    assert user.email == "old@example.com"

    updated_user = await user.update_email("new@example.com")
    assert len(fake_client.calls) == 2
    update_operation, update_request = fake_client.calls[1]
    assert update_operation == "UpdateEmail"
    assert isinstance(update_request, output_models.UpdateEmailRequest)
    assert update_request.user_id == "user-123"
    assert update_request.email == "new@example.com"
    assert updated_user.email == "new@example.com"
