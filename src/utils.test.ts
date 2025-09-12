import { expect, test } from "vitest";
import { getTokensEndpoint } from "./utils.js";

test("returns the correct endpoint URL", () => {
    expect(getTokensEndpoint("https://example.com")).toBe(
        "https://example.com/api/v1/trusted_publishing/tokens",
    );
});
