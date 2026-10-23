import { expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";

import { CodeInput, placeDigits, typedText } from "../../src/components/app/code-input";

const empty = ["", "", "", "", "", ""];

test("a typed digit fills its box and moves focus to the next one", () => {
  expect(placeDigits(empty, 0, "4")).toEqual({ digits: ["4", "", "", "", "", ""], focus: 1 });
});

test("a whole code fills every box from the first, whichever box it lands in", () => {
  expect(placeDigits(empty, 3, "123456")).toEqual({
    digits: ["1", "2", "3", "4", "5", "6"],
    focus: 5,
  });
  expect(placeDigits(empty, 0, " 123-456 ")?.digits.join("")).toBe("123456");
});

test("a partial paste fills from the box it lands in", () => {
  expect(placeDigits(["1", "2", "", "", "", ""], 2, "3456")).toEqual({
    digits: ["1", "2", "3", "4", "5", "6"],
    focus: 5,
  });
  expect(placeDigits(["1", "2", "", "", "", ""], 4, "789")).toEqual({
    digits: ["1", "2", "", "", "7", "8"],
    focus: 5,
  });
});

test("a paste longer than the code keeps the first digits", () => {
  expect(placeDigits(empty, 2, "12345678")?.digits.join("")).toBe("123456");
});

test("text without digits changes nothing", () => {
  expect(placeDigits(empty, 0, "abc")).toBeNull();
});

test("typing into a filled box keeps only the new digit", () => {
  expect(typedText("3", "35")).toBe("5");
  expect(typedText("3", "53")).toBe("5");
  expect(typedText("", "7")).toBe("7");
  expect(typedText("3", "123456")).toBe("123456");
});

test("the boxes are labelled and the first offers one-time-code autofill", () => {
  const html = renderToStaticMarkup(
    <CodeInput length={6} labelId="code-label" verify={async () => true} />,
  );
  expect(html).toContain('role="group"');
  expect(html).toContain('aria-labelledby="code-label"');
  expect(html.match(/autoComplete="one-time-code"|autocomplete="one-time-code"/g)).toHaveLength(1);
  expect(html).toContain('aria-label="Digit 1 of 6"');
  expect(html).toContain('aria-label="Digit 6 of 6"');
  expect(html).toContain('inputMode="numeric"');
});
