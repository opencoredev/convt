import type { ReactNode } from "react";

import { TextLink } from "./layout";

// A small Markdown reader for content/changelog.md: headings (#, ##, ###), paragraphs,
// "- " lists, and inline `code`, **bold** and [links](url). It builds React elements
// rather than HTML strings, so the file can never inject markup into the page.

export type Block =
  | { type: "heading"; level: 1 | 2 | 3; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] };

export function parseMarkdown(source: string): Block[] {
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let list: string[] | null = null;
  const flush = () => {
    if (paragraph.length) blocks.push({ type: "paragraph", text: paragraph.join(" ") });
    if (list) blocks.push({ type: "list", items: list });
    paragraph = [];
    list = null;
  };
  for (const raw of source.split("\n")) {
    const line = raw.trimEnd();
    const heading = /^(#{1,3})\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      blocks.push({
        type: "heading",
        level: heading[1].length as 1 | 2 | 3,
        text: heading[2],
      });
    } else if (/^[-*]\s+/.test(line)) {
      if (paragraph.length) flush();
      list ??= [];
      list.push(line.replace(/^[-*]\s+/, ""));
    } else if (/^\s+\S/.test(line) && list) {
      // Continuation of the previous list item.
      list[list.length - 1] += ` ${line.trim()}`;
    } else if (line.trim() === "") {
      flush();
    } else {
      if (list) flush();
      paragraph.push(line.trim());
    }
  }
  flush();
  return blocks;
}

/** Splits a section into release-sized groups at each level-2 heading. */
export function splitAtLevel2(blocks: Block[]) {
  const intro: Block[] = [];
  const sections: { title: string; blocks: Block[] }[] = [];
  for (const block of blocks) {
    if (block.type === "heading" && block.level === 2)
      sections.push({ title: block.text, blocks: [] });
    else if (sections.length) sections[sections.length - 1].blocks.push(block);
    else intro.push(block);
  }
  return { intro, sections };
}

const inlinePattern = /(`[^`]+`)|(\*\*[^*]+\*\*)|(\[[^\]]+\]\([^)\s]+\))/g;

export function Inline({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(inlinePattern)) {
    const index = match.index ?? 0;
    if (index > last) parts.push(text.slice(last, index));
    const token = match[0];
    if (token.startsWith("`"))
      parts.push(
        <code key={index} className="rounded bg-chip px-1 py-px font-mono text-[0.88em]">
          {token.slice(1, -1)}
        </code>,
      );
    else if (token.startsWith("**"))
      parts.push(
        <strong key={index} className="font-semibold text-ink">
          {token.slice(2, -2)}
        </strong>,
      );
    else {
      const [, label, href] = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(token) ?? [];
      const safe = /^(https:\/\/|\/|#|mailto:)/.test(href ?? "");
      parts.push(
        safe ? (
          <TextLink key={index} href={href}>
            {label}
          </TextLink>
        ) : (
          label
        ),
      );
    }
    last = index + token.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return <>{parts}</>;
}

export function Blocks({ blocks }: { blocks: Block[] }) {
  return (
    <>
      {blocks.map((block, i) => {
        if (block.type === "heading")
          return (
            <h3 key={i} className="pt-2 font-mono text-xs/4 text-ink-2 uppercase">
              {block.text}
            </h3>
          );
        if (block.type === "list")
          return (
            <ul key={i} className="flex flex-col gap-2.5">
              {block.items.map((item, j) => (
                <li key={j} className="flex gap-3 text-[15px]/6 text-ink">
                  <span
                    aria-hidden="true"
                    className="mt-[9px] size-1.5 shrink-0 rounded-[3px] bg-green"
                  />
                  <span>
                    <Inline text={item} />
                  </span>
                </li>
              ))}
            </ul>
          );
        return (
          <p key={i} className="text-[15px]/6 text-ink-2">
            <Inline text={block.text} />
          </p>
        );
      })}
    </>
  );
}
