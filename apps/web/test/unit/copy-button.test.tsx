import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { CopyButton } from "../../src/components/app/copy-button";

test("the copy control labels the action and does not print the secret in the button", () => {
  const html = renderToStaticMarkup(
    <CopyButton value="secret-value-for-test" aria-label="Copy API key" />,
  );
  expect(html).toContain("Copy API key");
  expect(html).toContain("Copy");
  expect(html).not.toContain("secret-value-for-test");
});
