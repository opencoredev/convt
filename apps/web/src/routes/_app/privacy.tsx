import { createFileRoute } from "@tanstack/react-router";

import { UnpublishedPage } from "#/components/app/unpublished-page";

// PLACEHOLDER: the Privacy page is not written yet. Linked from the sign-in screen and the landing footer.
export const Route = createFileRoute("/_app/privacy")({
  head: () => ({ meta: [{ title: "Privacy · convt" }] }),
  component: () => <UnpublishedPage title="Privacy" />,
});
