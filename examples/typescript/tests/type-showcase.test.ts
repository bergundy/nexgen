import { describe, expect, test, vi } from "vitest";

const runtime = vi.hoisted(() => {
  const startOperation = vi.fn();
  const createNexusServiceClient = vi.fn(() => ({
    startOperation,
  }));
  return {
    startOperation,
    createNexusServiceClient,
  };
});

vi.mock("@temporalio/workflow", async () => {
  const actual = await vi.importActual<typeof import("@temporalio/workflow")>(
    "@temporalio/workflow",
  );
  return {
    ...actual,
    createNexusServiceClient: runtime.createNexusServiceClient,
  };
});

import {
  GetUserRequest,
  User,
  UserCapability,
  UserStatus,
  TypeShowcase,
  getUser,
} from "../type-showcase/index.ts";

const userProfile = () => ({
  capabilities: UserCapability.ReadProfile | UserCapability.UpdateEmail,
  notificationTarget: { tag: "email" as const, value: "old@example.com" },
  syncState: { tag: "ok" as const, value: "synced" },
  address: {
    street: "1 Main St",
    city: "Portland",
    country: "US",
    coordinates: [45.5152, -122.6784] as [number, number],
  },
  metadata: { tier: "enterprise" },
  tags: ["admin", "beta"],
});

const userResource = (email: string, displayName: string) =>
  new User("user-123", email, displayName, UserStatus.Active, userProfile());

describe("type-showcase generated output", () => {
  test("exposes WIT-native type showcase metadata", () => {
    expect(TypeShowcase.name).toBe("TypeShowcase");
    expect(TypeShowcase.operations.getUser.name).toBe("GetUser");
    expect(TypeShowcase.operations.updateEmail.name).toBe("UpdateEmail");
    expect(TypeShowcase.operations.rename.name).toBe("Rename");
    expect(TypeShowcase.operations.deactivate.name).toBe("Deactivate");
    expect(GetUserRequest).toEqual({});
    expect(UserStatus.Active).toBe(0);
    expect(UserCapability.ReadProfile).toBe(1);
    expect(UserCapability.UpdateEmail).toBe(2);
  });

  test("passes WIT records directly and returns a user resource", async () => {
    runtime.startOperation
      .mockResolvedValueOnce({
        result: async () => userResource("old@example.com", "Old Name"),
      })
      .mockResolvedValueOnce({
        result: async () => userResource("new@example.com", "Old Name"),
      })
      .mockResolvedValueOnce({
        result: async () => userResource("new@example.com", "New Name"),
      })
      .mockResolvedValueOnce({
        result: async () => undefined,
      });

    const user = await getUser({
      userId: "user-123",
      consistencyToken: "read-123",
    });

    expect(user).toBeInstanceOf(User);
    expect(user.userId).toBe("user-123");
    expect(user.email).toBe("old@example.com");
    expect(user.displayName).toBe("Old Name");
    expect(user.status).toBe(UserStatus.Active);
    expect(user.profile.capabilities & UserCapability.ReadProfile).toBeTruthy();
    expect(user.profile.syncState).toEqual({ tag: "ok", value: "synced" });
    expect(user.profile.notificationTarget).toEqual({
      tag: "email",
      value: "old@example.com",
    });
    expect(user.profile.address?.coordinates).toEqual([45.5152, -122.6784]);
    expect(user.profile.metadata).toEqual({ tier: "enterprise" });
    expect(user.profile.tags).toEqual(["admin", "beta"]);
    expect(runtime.createNexusServiceClient).toHaveBeenCalledWith({
      service: TypeShowcase,
      endpoint: "__type_showcase",
    });
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      1,
      TypeShowcase.operations.getUser,
      {
        consistencyToken: "read-123",
        userId: "user-123",
      },
    );

    const updatedUser = await user.updateEmail("new@example.com");
    expect(updatedUser).toBeInstanceOf(User);
    expect(updatedUser.email).toBe("new@example.com");
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      2,
      TypeShowcase.operations.updateEmail,
      {
        userId: "user-123",
        email: "new@example.com",
      },
    );

    const renamedUser = await updatedUser.rename("New Name");
    expect(renamedUser).toBeInstanceOf(User);
    expect(renamedUser.displayName).toBe("New Name");
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      3,
      TypeShowcase.operations.rename,
      {
        userId: "user-123",
        displayName: "New Name",
      },
    );

    await renamedUser.deactivate("requested");
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      4,
      TypeShowcase.operations.deactivate,
      {
        userId: "user-123",
        reason: "requested",
      },
    );
  });
});
