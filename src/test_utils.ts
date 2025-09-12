import type { RequestHandler } from "msw";
import * as msw from "msw/node";

export function startMockServer(handlers: RequestHandler[]): msw.SetupServer {
    const server = msw.setupServer(...handlers);
    server.listen({ onUnhandledRequest: "error" });
    return server;
}

export const AUDIENCE = "my-crates.io";
export const REGISTRY_URL = `https://${AUDIENCE}`;

/** Set up environment variable to pass the permissions check */
export function setRegistryUrl(): void {
    // Set the registry URL in the environment variable
    process.env.ACTIONS_ID_TOKEN_REQUEST_URL = REGISTRY_URL;
}
