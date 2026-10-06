import { createFileRoute } from "@tanstack/react-router";

import { ApiReference, type OpenApiDocument } from "#/components/site/openapi";
import { cx } from "#/components/app/ui";
import { siteColumn } from "#/components/site/layout";
import { routes, seo } from "#/lib/site";
import placeholder from "../../../../content/openapi.placeholder.json";

// The real spec is src/generated/openapi.json, which the API work (plan P9) generates
// from convt-server. The glob resolves at build time: when that file exists the build
// renders it, otherwise the placeholder in content/, and the page says so.
const generated = import.meta.glob<OpenApiDocument>("../../../generated/openapi.json", {
  eager: true,
  import: "default",
});
const doc = (Object.values(generated)[0] ?? placeholder) as OpenApiDocument;

export const Route = createFileRoute("/_site/docs/api")({
  head: () =>
    seo({
      title: "API reference · convt",
      description:
        "Convert files from your own code with the convt API: create a job, upload, poll and download. Pay per conversion.",
      path: routes.apiDocs,
    }),
  component: ApiDocsPage,
});

function ApiDocsPage() {
  return (
    <>
      {doc["x-convt-placeholder"] && (
        <div className={cx(siteColumn, "pt-8")}>
          <p
            role="note"
            className="rounded-xl bg-[#fff8e6] px-4 py-3 text-sm/5 text-[#5c4300] shadow-[inset_0_0_0_1px_#f0dca6] dark:bg-[#2a2210] dark:text-[#f0d48a] dark:shadow-[inset_0_0_0_1px_#4a3c17]"
          >
            <span className="font-semibold">Preview.</span> The API is not live yet. This reference
            shows its planned shape and may change before launch.
          </p>
        </div>
      )}
      <ApiReference doc={doc} />
    </>
  );
}
