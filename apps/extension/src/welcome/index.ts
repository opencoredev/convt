// welcome.html: the page that opens after install, and (with ?access=<job>) the page
// that asks for site access when a host won't share an image.

import { parseJobId } from "../shared/jobs.ts";
import { renderAccess } from "./access.ts";
import { renderOnboarding } from "./onboarding.ts";

const app = document.getElementById("app");
const accessJob = parseJobId(new URLSearchParams(location.search).get("access"));
if (app) void (accessJob ? renderAccess(app, accessJob) : renderOnboarding(app));
