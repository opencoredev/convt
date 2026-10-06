import { createStart } from "@tanstack/react-start";

import { requestMiddleware } from "./server/request";

export const startInstance = createStart(() => ({ requestMiddleware: [requestMiddleware] }));
