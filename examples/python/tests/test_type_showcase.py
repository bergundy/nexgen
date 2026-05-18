from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

from nexusrpc import Operation
import pytest
import temporalio.workflow

APP_ROOT = Path(__file__).resolve().parent
OUTPUT_PATH = APP_ROOT.parent / "type_showcase"

import type_showcase as output
import type_showcase.models as output_models

GET_USER_OPERATION = output.__nexus_operation_registry__[("TypeShowcase", "GetUser")]
UPDATE_EMAIL_OPERATION = output.__nexus_operation_registry__[
    ("TypeShowcase", "UpdateEmail")
]
RENAME_OPERATION = output.__nexus_operation_registry__[("TypeShowcase", "Rename")]
DEACTIVATE_OPERATION = output.__nexus_operation_registry__[
    ("TypeShowcase", "Deactivate")
]


def user_profile() -> output_models.UserProfile:
    return output_models.UserProfile(
        capabilities=output_models.UserCapability.ReadProfile
        | output_models.UserCapability.UpdateEmail,
        notification_target=("email", "old@example.com"),
        sync_state=("ok", "synced"),
        address=output_models.PostalAddress(
            street="1 Main St",
            city="Portland",
            country="US",
            coordinates=(45.5152, -122.6784),
        ),
        metadata={"tier": "enterprise"},
        tags=["admin", "beta"],
    )


def user_resource(
    *,
    email: str,
    display_name: str,
) -> output.User:
    return output.User(
        user_id="user-123",
        email=email,
        display_name=display_name,
        status=output_models.UserStatus.Active,
        profile=user_profile(),
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
            assert input.consistency_token == "read-123"
            return FakeOperationHandle(
                user_resource(email="old@example.com", display_name="Old Name")
            )

        if operation == "UpdateEmail":
            assert output_type is output.User
            assert isinstance(input, output_models.UpdateEmailRequest)
            assert input.user_id == "user-123"
            assert input.email == "new@example.com"
            return FakeOperationHandle(
                user_resource(email=input.email, display_name="Old Name")
            )

        if operation == "Rename":
            assert output_type is output.User
            assert isinstance(input, output_models.RenameRequest)
            assert input.user_id == "user-123"
            assert input.display_name == "New Name"
            return FakeOperationHandle(
                user_resource(email="new@example.com", display_name=input.display_name)
            )

        assert operation == "Deactivate"
        assert output_type is None
        assert isinstance(input, output_models.DeactivateRequest)
        assert input.user_id == "user-123"
        assert input.reason == "requested"
        return FakeOperationHandle(None)


@pytest.fixture
def fake_client(monkeypatch: pytest.MonkeyPatch) -> FakeNexusClient:
    fake_client = FakeNexusClient()

    def fake_create_nexus_client(*, service: str, endpoint: str) -> FakeNexusClient:
        assert service == "TypeShowcase"
        assert endpoint == "__type_showcase"
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
    assert registry[("TypeShowcase", "GetUser")] is GET_USER_OPERATION
    assert isinstance(UPDATE_EMAIL_OPERATION, Operation)
    assert UPDATE_EMAIL_OPERATION.name == "UpdateEmail"
    assert registry[("TypeShowcase", "UpdateEmail")] is UPDATE_EMAIL_OPERATION
    assert isinstance(RENAME_OPERATION, Operation)
    assert RENAME_OPERATION.name == "Rename"
    assert registry[("TypeShowcase", "Rename")] is RENAME_OPERATION
    assert isinstance(DEACTIVATE_OPERATION, Operation)
    assert DEACTIVATE_OPERATION.name == "Deactivate"
    assert registry[("TypeShowcase", "Deactivate")] is DEACTIVATE_OPERATION
    assert not hasattr(output, "TypeShowcase")
    assert hasattr(output, "User")
    assert not hasattr(output_models, "DeactivateResponse")
    assert not hasattr(output_models.GetUserRequest, "to_proto")
    assert output_models.UserStatus.Active == 0
    assert output_models.UserCapability.ReadProfile == 1
    assert output_models.UserCapability.UpdateEmail == 2


def test_generated_wit_native_models_cover_common_wit_shapes() -> None:
    profile = user_profile()

    assert profile.notification_target == ("email", "old@example.com")
    assert profile.capabilities == (
        output_models.UserCapability.ReadProfile
        | output_models.UserCapability.UpdateEmail
    )
    assert profile.sync_state == ("ok", "synced")
    assert profile.address is not None
    assert profile.address.coordinates == (45.5152, -122.6784)
    assert profile.metadata == {"tier": "enterprise"}
    assert profile.tags == ["admin", "beta"]


async def test_get_user_returns_wit_user_resource(
    fake_client: FakeNexusClient,
) -> None:
    user = await output.get_user(
        user_id="user-123",
        consistency_token="read-123",
    )

    assert len(fake_client.calls) == 1
    assert isinstance(user, output.User)
    assert user.user_id == "user-123"
    assert user.email == "old@example.com"
    assert user.display_name == "Old Name"
    assert user.status is output_models.UserStatus.Active
    assert user.profile.notification_target == ("email", "old@example.com")
    assert user.profile.sync_state == ("ok", "synced")
    assert (
        user.profile.capabilities & output_models.UserCapability.ReadProfile
    ) == output_models.UserCapability.ReadProfile

    updated_user = await user.update_email("new@example.com")
    assert len(fake_client.calls) == 2
    update_operation, update_request = fake_client.calls[1]
    assert update_operation == "UpdateEmail"
    assert isinstance(update_request, output_models.UpdateEmailRequest)
    assert update_request.user_id == "user-123"
    assert update_request.email == "new@example.com"
    assert updated_user.email == "new@example.com"
    assert updated_user.display_name == "Old Name"

    renamed_user = await updated_user.rename("New Name")
    assert len(fake_client.calls) == 3
    rename_operation, rename_request = fake_client.calls[2]
    assert rename_operation == "Rename"
    assert isinstance(rename_request, output_models.RenameRequest)
    assert rename_request.user_id == "user-123"
    assert rename_request.display_name == "New Name"
    assert renamed_user.email == "new@example.com"
    assert renamed_user.display_name == "New Name"

    await renamed_user.deactivate(reason="requested")
    assert len(fake_client.calls) == 4
    deactivate_operation, deactivate_request = fake_client.calls[3]
    assert deactivate_operation == "Deactivate"
    assert isinstance(deactivate_request, output_models.DeactivateRequest)
    assert deactivate_request.user_id == "user-123"
    assert deactivate_request.reason == "requested"
