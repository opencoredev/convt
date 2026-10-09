// Renders the real toast card in every state, for screenshots. Test build only.

import type { JobId, Problem, ToastView } from "../../src/shared/jobs.ts";
import { createCard } from "../../src/ui/toast-card.ts";

const jobId = "3b241101-e2bb-4255-8caf-4136c566a962" as JobId;
const thumb = "images/miso.webp";
const problems: Problem[] = [
  { kind: "blocked", host: "images.example-cdn.com" },
  { kind: "unreadable", from: "heic" },
  { kind: "http", status: 403 },
  { kind: "network" },
  { kind: "not-image" },
  { kind: "too-large" },
  { kind: "save-failed", reason: "FILE_NO_SPACE" },
  { kind: "clipboard" },
];
const views: Exclude<ToastView, { phase: "dismissed" }>[] = [
  { phase: "working", jobId, thumb, action: { kind: "save", target: "png" } },
  {
    phase: "saved",
    jobId,
    thumb,
    name: "miso-on-the-windowsill.png",
    bytes: 612_000,
    from: "webp",
    target: "png",
    downloadId: 1,
    note: null,
  },
  {
    phase: "saved",
    jobId,
    thumb,
    name: "dancing-cat.png",
    bytes: 48_200,
    from: "gif",
    target: "png",
    downloadId: 1,
    note: { kind: "animated", from: "gif" },
  },
  { phase: "copied", jobId, thumb, bytes: 1_240_000, from: "avif", note: { kind: "tip" } },
  ...problems.map((problem) => ({
    phase: "failed" as const,
    jobId,
    thumb,
    action: { kind: "save" as const, target: "jpg" as const },
    problem,
  })),
];

const grid = document.getElementById("grid");
for (const view of views) {
  const card = createCard({
    srcUrl: view.phase === "failed" && view.problem.kind === "network" ? "missing.png" : thumb,
    inline: false,
    handlers: { onAction: () => {}, onClose: () => {} },
  });
  card.update(view);
  card.element.dataset.open = "true";
  grid?.append(card.element);
}
