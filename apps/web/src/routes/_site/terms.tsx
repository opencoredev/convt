import { createFileRoute } from "@tanstack/react-router";

import { TextLink } from "#/components/site/layout";
import { LegalPage, List, Placeholder, type LegalSection } from "#/components/site/legal";
import { GITHUB_URL, SUPPORT_EMAIL, legal, routes, seo } from "#/lib/site";

// DRAFT pending Leo's legal review. Prices and plan facts match the pricing section
// and docs/plan.md; the refund policy is still a placeholder (plan P12).

export const Route = createFileRoute("/_site/terms")({
  head: () =>
    seo({
      title: "Terms of service · convt",
      description: "The terms for using convt, buying a Desktop license or Pro, and the convt API.",
      path: routes.terms,
    }),
  component: () => (
    <LegalPage
      title="Terms of service"
      summary={
        <p>
          These terms cover buying and using convt: the desktop app, convt Pro, convt.app and the
          API. The source code itself is licensed separately under the AGPL.
        </p>
      }
      sections={sections}
    />
  ),
});

const strong = "font-medium text-ink";

const sections: LegalSection[] = [
  {
    id: "agreement",
    title: "Agreement",
    body: (
      <p>
        convt is provided by <Placeholder>{legal.entity}</Placeholder> ("we"). By buying a license,
        starting a trial, creating an account or using the API, you agree to these terms. If you use
        convt for an organization, you accept them on its behalf.
      </p>
    ),
  },
  {
    id: "open-source",
    title: "Open source",
    body: (
      <p>
        convt's source code is free software under the GNU Affero General Public License, version 3
        (<TextLink href={GITHUB_URL}>on GitHub</TextLink>). Nothing in these terms limits your
        rights under that license. These terms cover the builds we sign and distribute, the license
        keys that unlock them, and our hosted services.
      </p>
    ),
  },
  {
    id: "plans",
    title: "Trials, licenses and subscriptions",
    body: (
      <List
        items={[
          <>
            <span className={strong}>Trial.</span> The app works fully for 7 days from your first
            conversion. convt Pro starts with a 7-day trial that needs a card and becomes a paid
            subscription unless you cancel before it ends. Each account gets one Pro trial.
          </>,
          <>
            <span className={strong}>Desktop license.</span> $29, paid once. It covers every version
            released within 12 months of purchase, on macOS, Windows and Linux, and you can keep
            using those versions for as long as you like.
          </>,
          <>
            <span className={strong}>convt Pro.</span> $12 a month, or $96 a year ($8 a month). It
            includes everything in Desktop, every update while you subscribe, and cloud conversions.
            It renews until you cancel; cancelling takes effect at the end of the period you paid
            for. After Pro ends, your last key keeps working for every version it covered.
          </>,
          <>
            <span className={strong}>API.</span> Billed monthly in arrears for the conversions you
            run, at the price on the dashboard when they run, up to the spend cap you set.
          </>,
        ]}
      />
    ),
  },
  {
    id: "payment",
    title: "Payment, taxes and refunds",
    body: (
      <>
        <p>
          Polar is the merchant of record for every purchase: you buy from Polar, which takes
          payment, charges any sales tax or VAT, and issues your receipt under its own terms.
        </p>
        <p>
          Refunds: <Placeholder>[Refund policy to be decided]</Placeholder>. Refunding a purchase
          revokes its license key, and the dashboard marks it as refunded.
        </p>
      </>
    ),
  },
  {
    id: "keys",
    title: "License keys",
    body: (
      <p>
        A license key is for you, or for one person in your organization. Do not share it publicly
        or resell it. We may revoke keys that were refunded, charged back or published.
      </p>
    ),
  },
  {
    id: "cloud",
    title: "Cloud conversions and fair use",
    body: (
      <>
        <p>
          You keep all rights to the files you convert. You give us permission to process them only
          to perform the conversion, and we delete them 24 hours after each job is created.
        </p>
        <List
          items={[
            "Pro includes up to 50 GB of input per month, with files up to 2 GB each. The dashboard shows what you have used.",
            "Only send files you have the right to convert. Do not use convt to process unlawful content, to attack or overload our systems, or to get around these limits.",
            "We may suspend cloud access or API keys that break these rules, and will tell you why.",
          ]}
        />
      </>
    ),
  },
  {
    id: "account",
    title: "Your account",
    body: (
      <p>
        Keep access to your email account and your API keys secure; you are responsible for what
        happens under them. You can delete your account from the dashboard at any time.
      </p>
    ),
  },
  {
    id: "warranty",
    title: "Warranty and liability",
    body: (
      <>
        <p>
          We work to make every conversion correct, but convt is provided as is. Keep your
          originals: convt never overwrites the files it reads, and you should check important
          output before relying on it.
        </p>
        <p>
          To the extent the law allows, we are not liable for indirect or consequential losses, and
          our total liability is limited to what you paid us in the 12 months before the claim.{" "}
          <Placeholder>[Liability terms to confirm]</Placeholder>
        </p>
      </>
    ),
  },
  {
    id: "law",
    title: "Changes and governing law",
    body: (
      <>
        <p>
          We will announce material changes to these terms on this page and by email before they
          apply. These terms are governed by <Placeholder>{legal.jurisdiction}</Placeholder>.
        </p>
        <p>
          Questions: <TextLink href={`mailto:${SUPPORT_EMAIL}`}>{SUPPORT_EMAIL}</TextLink>.
        </p>
      </>
    ),
  },
];
