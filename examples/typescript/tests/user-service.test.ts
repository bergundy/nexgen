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
  UserService,
  getUser,
} from "../user-service/index.ts";

const userResource = (email: string) => new User("user-123", email);

describe("user-service generated output", () => {
  test("exposes basic WIT-native service metadata", () => {
    expect(UserService.name).toBe("UserService");
    expect(UserService.operations.getUser.name).toBe("GetUser");
    expect(UserService.operations.updateEmail.name).toBe("UpdateEmail");
    expect(GetUserRequest).toEqual({});
  });

  test("passes WIT records directly and returns a user resource", async () => {
    runtime.startOperation
      .mockResolvedValueOnce({
        result: async () => userResource("old@example.com"),
      })
      .mockResolvedValueOnce({
        result: async () => userResource("new@example.com"),
      });

    const user = await getUser({ userId: "user-123" });

    expect(user).toBeInstanceOf(User);
    expect(user.userId).toBe("user-123");
    expect(user.email).toBe("old@example.com");
    expect(runtime.createNexusServiceClient).toHaveBeenCalledWith({
      service: UserService,
      endpoint: "__user_service",
    });
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      1,
      UserService.operations.getUser,
      {
        userId: "user-123",
      },
    );

    const updatedUser = await user.updateEmail("new@example.com");
    expect(updatedUser).toBeInstanceOf(User);
    expect(updatedUser.email).toBe("new@example.com");
    expect(runtime.startOperation).toHaveBeenNthCalledWith(
      2,
      UserService.operations.updateEmail,
      {
        userId: "user-123",
        email: "new@example.com",
      },
    );
  });
});
