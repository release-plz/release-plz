import * as core from "@actions/core";
import { getAudienceFromUrl, getRegistryUrl } from "./registry_url.js";
import {
    getTokensEndpoint,
    runAction,
    throwHttpErrorMessage,
    TOKEN_KEY,
    REGISTRY_URL_KEY,
    getUserAgent,
} from "./utils.js";

runAction(run);

export async function run(): Promise<void> {
    checkPermissions();

    const registryUrl = getRegistryUrl();

    const audience = getAudienceFromUrl(registryUrl);

    // Get the GitHub Actions JWT token, used to prove where the GitHub workflow is running.
    const jwtToken = await getJwtToken(audience);

    // Retrieve the temporary token from the Cargo registry.
    const token = await requestTrustedPublishingToken(registryUrl, jwtToken);

    // Set the token as output, so that users can access it in subsequent workflow steps.
    setTokenOutput(token);

    // Store state used in the post job to revoke the token.
    core.saveState(TOKEN_KEY, token);
    core.saveState(REGISTRY_URL_KEY, registryUrl);
}

/** Check that GitHub Actions workflow permissions are set correctly. */
function checkPermissions(): void {
    if (
        process.env.ACTIONS_ID_TOKEN_REQUEST_URL === undefined ||
        !process.env.ACTIONS_ID_TOKEN_REQUEST_URL
    ) {
        throw new Error(
            "Please ensure the 'id-token' permission is set to 'write' in your workflow. For more information, see: https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect#adding-permissions-settings",
        );
    }
}

async function getJwtToken(audience: string): Promise<string> {
    core.info(`Retrieving GitHub Actions JWT token with audience: ${audience}`);

    const jwtToken = await core.getIDToken(audience);

    if (!jwtToken) {
        throw new Error("Failed to retrieve JWT token from GitHub Actions");
    }

    core.info("Retrieved JWT token successfully");
    return jwtToken;
}

async function requestTrustedPublishingToken(
    registryUrl: string,
    jwtToken: string,
): Promise<string> {
    const tokenUrl = getTokensEndpoint(registryUrl);
    const userAgent = getUserAgent();
    core.info(
        `Requesting token from: ${tokenUrl}. User agent: ${userAgent["User-Agent"]}`,
    );

    const response = await fetch(tokenUrl, {
        method: "POST",
        headers: {
            /* eslint-disable  @typescript-eslint/naming-convention */
            "Content-Type": "application/json",
            ...userAgent,
        },
        body: JSON.stringify({ jwt: jwtToken }),
    });

    if (!response.ok) {
        // Status is not in the range 200-299.
        await throwHttpErrorMessage(
            "Failed to retrieve token from Cargo registry",
            response,
        );
    }
    const tokenResponse = (await response.json()) as { token: string };

    if (!tokenResponse.token) {
        await throwHttpErrorMessage(
            "Failed to retrieve token from the Cargo registry response body",
            response,
        );
    }

    core.info("Retrieved token successfully");

    return tokenResponse.token;
}

function setTokenOutput(token: string): void {
    // Register the token with the runner as a secret to ensure it is masked in the logs.
    core.setSecret(token);
    core.setOutput(TOKEN_KEY, token);
}
