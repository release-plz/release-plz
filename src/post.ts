import * as core from "@actions/core";
import {
    getTokensEndpoint,
    runAction,
    TOKEN_KEY,
    REGISTRY_URL_KEY,
    throwHttpErrorMessage,
    getUserAgent,
} from "./utils.js";

runAction(cleanup);

export async function cleanup(): Promise<void> {
    // Retrieve the token and registry URL from the action state.
    // These values are set in the main job.
    const token = core.getState(TOKEN_KEY);
    const registryUrl = core.getState(REGISTRY_URL_KEY);

    if (!token) {
        // Probably the action terminated before the token was retrieved.
        core.info("No token to revoke");
        return;
    }

    // Revoke token so that even if it's leaked, it cannot be used anymore.
    await revokeToken(registryUrl, token);
}

async function revokeToken(registryUrl: string, token: string): Promise<void> {
    const tokensEndpoint = getTokensEndpoint(registryUrl);
    core.info(`Revoking trusted publishing token at ${tokensEndpoint}`);

    const response = await fetch(tokensEndpoint, {
        method: "DELETE",
        headers: {
            /* eslint-disable  @typescript-eslint/naming-convention */
            Authorization: `Bearer ${token}`,
            ...getUserAgent(),
        },
    });

    if (!response.ok) {
        await throwHttpErrorMessage("Failed to revoke token", response);
    }

    core.info("Token revoked successfully");
}
