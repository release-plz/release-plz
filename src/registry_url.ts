import * as core from "@actions/core";

// Extract audience from registry URL by removing `https://` or `http://`.
export function getAudienceFromUrl(url: string): string {
    const audience = url.replace(/^https?:\/\//, "");

    if (audience.startsWith("http://") || audience.startsWith("https://")) {
        throw new Error(
            "Bug: The audience should not include the protocol (http:// or https://).",
        );
    }

    return audience;
}

export function getRegistryUrl(): string {
    const url = core.getInput("url") || "https://crates.io";

    // Remove trailing `/` at the end of the URL if present.
    if (url.endsWith("/")) {
        return url.slice(0, -1);
    }

    return url;
}
