import { describe, expect, it } from "vitest"
import { isProfileNotFoundError, loadOptionalProfile } from "./profileLoad"

describe("profileLoad", () => {
  it("treats validation errors for the profile field as missing profiles", async () => {
    const result = await loadOptionalProfile(async () => {
      throw {
        code: "validation_error",
        message: "Business profile not found",
        details: [{ field: "businessProfile", message: "Invalid value", code: "invalid_value" }],
      }
    }, "businessProfile")

    expect(result).toEqual({ profile: null, failed: false })
  })

  it("treats storage errors as load failures", async () => {
    const result = await loadOptionalProfile(async () => {
      throw { code: "storage_error", message: "Database failed" }
    }, "businessProfile")

    expect(result).toEqual({ profile: null, failed: true })
  })

  it("detects profile-not-found validation errors", () => {
    expect(
      isProfileNotFoundError(
        {
          code: "validation_error",
          details: [{ field: "taxProfile", message: "x", code: "invalid_value" }],
        },
        "taxProfile",
      ),
    ).toBe(true)
    expect(
      isProfileNotFoundError({ code: "storage_error", message: "fail" }, "taxProfile"),
    ).toBe(false)
  })
})
