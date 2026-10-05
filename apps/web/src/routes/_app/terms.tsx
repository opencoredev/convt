import { createFileRoute } from "@tanstack/react-router";

import { UnpublishedPage } from "#/components/app/unpublished-page";

// PLACEHOLDER: the Terms page is not written yet. Linked from the sign-in screen and the landing footer.
export const Route = createFileRoute("/_app/terms")({
  head: () => ({ meta: [{ title: "Terms · convt" }] }),
  component: () => <UnpublishedPage title="Terms" />,
});
