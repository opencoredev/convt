// The site access page (welcome.html?access=<job>): opened from a toast when an image
// host won't share an image until the user allows it.

import type { JobId } from "../shared/jobs.ts";
import { hostOf, hostPattern } from "../shared/hosts.ts";
import type { ToBackground } from "../shared/messages.ts";
import { getPending } from "../shared/store.ts";
import { h, mark } from "../ui/dom.ts";

const ALL_SITES = { origins: ["<all_urls>"] };

function expired(): HTMLElement {
  return h("main", { class: "access" }, [
    mark(32),
    h("h1", {}, ["This request has expired"]),
    h("p", {}, ["Right-click the image again and pick a format. convt will pick up from there."]),
  ]);
}

export async function renderAccess(root: HTMLElement, jobId: JobId) {
  document.title = "Allow access · convt";
  const job = await getPending(jobId);
  if (job === null) {
    root.replaceChildren(expired());
    return;
  }

  const host = hostOf(job.srcUrl);
  const pattern = hostPattern(job.srcUrl);
  if (host === null || pattern === null) {
    root.replaceChildren(expired());
    return;
  }
  const notice = h("p", { class: "notice", role: "status" });
  const say = (text: string, tone: "info" | "error" = "info") => {
    notice.textContent = text;
    notice.dataset.tone = tone;
  };

  const finish = () => {
    say("Access granted. Finishing your image…");
    const message: ToBackground = { kind: "access-granted", jobId };
    // The background worker returns you to your tab and closes this one.
    void chrome.runtime.sendMessage(message);
  };

  const ask = async (origins: string[]) => {
    say("");
    const granted = await chrome.permissions.request({ origins }).catch(() => false);
    if (granted) finish();
    else say("Chrome didn't grant access. You can try again whenever you like.", "error");
  };

  const all = h("button", { class: "btn btn-primary", type: "button" }, ["Allow on all sites"]);
  all.addEventListener("click", () => void ask(ALL_SITES.origins));
  const one = h("button", { class: "btn", type: "button" }, [`Only on ${host}`]);
  one.addEventListener("click", () => void ask([pattern]));

  const image = h("img", { src: job.srcUrl, alt: "" });
  image.addEventListener("error", () => image.remove());

  root.replaceChildren(
    h("main", { class: "access" }, [
      mark(32),
      h("h1", {}, ["Let convt read this image"]),
      h("figure", { class: "access-image" }, [
        image,
        h("figcaption", { class: "access-host mono" }, [host]),
      ]),
      h("p", {}, [
        `${host} only shares its images with extensions you allow. Chrome will ask you to confirm.`,
      ]),
      h("div", { class: "access-actions" }, [all, one]),
      notice,
      h("p", { class: "fine" }, [
        "Chrome calls this reading and changing your data on those sites. convt only downloads images you right-click, runs on a page only when you use it, and never uploads anything.",
      ]),
    ]),
  );

  if (await chrome.permissions.contains({ origins: [pattern] })) finish();
}
