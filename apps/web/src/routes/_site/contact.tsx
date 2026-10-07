import { createFileRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { cx } from "#/components/app/ui";
import { PageHeader, TextLink, siteColumn } from "#/components/site/layout";
import { GITHUB_URL, PRIVACY_EMAIL, SUPPORT_EMAIL, routes, seo } from "#/lib/site";

export const Route = createFileRoute("/_site/contact")({
  head: () =>
    seo({
      title: "Contact and help · convt",
      description: "Get help with convt: support email, license keys, billing and bug reports.",
      path: routes.contact,
    }),
  component: ContactPage,
});

const channels: { title: string; body: ReactNode; action: { label: string; href: string } }[] = [
  {
    title: "Email support",
    body: "Questions about the app, a conversion that went wrong, or your purchase.",
    action: { label: SUPPORT_EMAIL, href: `mailto:${SUPPORT_EMAIL}` },
  },
  {
    title: "Bugs and feature requests",
    body: "convt is open source. Report bugs and suggest formats in the issue tracker.",
    action: { label: "GitHub issues", href: `${GITHUB_URL}/issues` },
  },
  {
    title: "Privacy requests",
    body: "Ask for a copy of your data, or anything else about how we handle it.",
    action: { label: PRIVACY_EMAIL, href: `mailto:${PRIVACY_EMAIL}` },
  },
];

const questions: { q: string; a: ReactNode }[] = [
  {
    q: "Where is my license key?",
    a: (
      <>
        It was emailed to you after you paid. You can also{" "}
        <TextLink href={routes.signIn}>sign in</TextLink> with the email address you bought with:
        every key you own is on the dashboard, including purchases made before you had an account.
      </>
    ),
  },
  {
    q: "Does convt upload my files?",
    a: (
      <>
        No. The app converts on your computer. With convt Pro you can choose to convert a file in
        the cloud, one job at a time, and those files are deleted after 24 hours. The{" "}
        <TextLink href={routes.privacy}>privacy policy</TextLink> lists every connection the app
        makes.
      </>
    ),
  },
  {
    q: "How does the free trial work?",
    a: "Click Start 7-day trial in the app and sign in to convt.app; the app then works fully for 7 days, with no card. Each account and each computer gets one trial. convt Pro has its own 7-day trial when you subscribe.",
  },
  {
    q: "What happens when my 12 months of updates end?",
    a: "Your Desktop license keeps working with every version released in those 12 months, for as long as you use them. Newer versions need a new license or Pro.",
  },
  {
    q: "How do I cancel or change convt Pro?",
    a: (
      <>
        On the dashboard under Billing you can switch between monthly and yearly, cancel, or open
        your receipts. Cancelling keeps Pro until the end of the period you paid for.
      </>
    ),
  },
  {
    q: "Why do Word or Excel files need a download?",
    a: (
      <>
        Documents convert through LibreOffice, which comes as an optional document pack so everyone
        else gets a smaller app. See the <TextLink href={routes.formats}>formats</TextLink> page.
      </>
    ),
  },
];

function ContactPage() {
  return (
    <div className={cx(siteColumn, "flex flex-col gap-16 pt-12 pb-20 md:pt-16")}>
      <PageHeader eyebrow="Help" title="Contact and help">
        <p>Most answers are below. If yours is not, email us.</p>
      </PageHeader>

      <ul className="grid gap-4 sm:grid-cols-2">
        {channels.map((channel) => (
          <li
            key={channel.title}
            className="flex flex-col gap-4 rounded-2xl bg-raised p-5 shadow-[inset_0_0_0_1px_var(--line)] dark:bg-panel"
          >
            <div className="flex flex-col gap-1.5">
              <h2 className="text-base/6 font-semibold">{channel.title}</h2>
              <p className="text-sm/[21px] text-ink-2">{channel.body}</p>
            </div>
            <TextLink href={channel.action.href} className="mt-auto self-start text-sm/5">
              {channel.action.label}
            </TextLink>
          </li>
        ))}
      </ul>

      <section
        aria-labelledby="faq-title"
        className="grid gap-6 md:grid-cols-[220px_1fr] md:gap-10"
      >
        <h2 id="faq-title" className="text-2xl/8 font-semibold tracking-[-0.02em]">
          Common questions
        </h2>
        <dl className="flex max-w-[680px] flex-col divide-y divide-divider border-y border-line">
          {questions.map(({ q, a }) => (
            <div key={q} className="flex flex-col gap-2 py-5">
              <dt className="text-[15px]/6 font-medium">{q}</dt>
              <dd className="text-[15px]/6 text-ink-2">{a}</dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}
