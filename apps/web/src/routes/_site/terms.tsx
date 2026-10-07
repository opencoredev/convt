import { createFileRoute } from "@tanstack/react-router";

import { TextLink } from "#/components/site/layout";
import { LegalPage, List, type LegalSection } from "#/components/site/legal";
import { GITHUB_URL, SUPPORT_EMAIL, legal, routes, seo } from "#/lib/site";

// Prices and plan facts match the pricing section and docs/plan.md; refunds and key
// revocation match docs/p7-billing-plan.md. The seller name, refund window, governing
// law and courts come from `legal` in lib/site.ts.

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
        These terms are an agreement between you and {legal.entity} ("we", "us"), the name convt is
        sold under. Every purchase is processed by Polar as merchant of record (see Payment, taxes
        and refunds). By buying a license, starting a trial, creating an account or using the API,
        you agree to these terms. If you use convt for an organization, you accept them on its
        behalf.
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
          Purchases are processed by Polar (<TextLink href="https://polar.sh">polar.sh</TextLink>)
          as merchant of record: you buy from Polar, which handles payment, sales tax and VAT, and
          invoicing, and issues your receipt under its own terms.
        </p>
        <p>
          If convt is not right for you, email {SUPPORT_EMAIL} within {legal.refundDays} days of
          paying for a Desktop license or Pro, including a Pro renewal, and we will refund it in
          full. You do not need to give a reason.
        </p>
        <p>
          Polar sends every refund to the payment method you used. A refund in full revokes the
          license key that payment bought, and the dashboard marks it as refunded. Keys are checked
          offline, so revoking one cannot switch off a copy that already uses it; by taking the
          refund you agree to stop using that key. A refund of a Pro payment does not cancel the
          subscription, so cancel it on the dashboard as well. API usage pays for conversions that
          already ran, so we refund it only when we billed it wrongly. None of this limits the
          rights consumer law gives you where you live.
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
          our total liability is limited to what you paid us in the 12 months before the claim.
        </p>
        <p>
          Nothing in these terms limits liability that the law does not let us limit, such as for
          fraud or for death or personal injury caused by negligence, or takes away your statutory
          rights as a consumer.
        </p>
      </>
    ),
  },
  {
    id: "termination",
    title: "Ending your account",
    body: (
      <>
        <p>
          You can stop using convt at any time, and delete your account from Settings on the
          dashboard. Deleting it first ends any subscription, without a refund for the rest of the
          period, then removes the account; this usually takes minutes, but takes longer if a
          billing step has to be retried. Desktop and Pro keys you paid for keep working offline for
          every version they cover.
        </p>
        <p>
          We may suspend or close an account, or revoke API keys and cloud access, if you seriously
          or repeatedly break these terms, if a payment is charged back, or if the law requires it.
          We will tell you why unless the law stops us.
        </p>
      </>
    ),
  },
  {
    id: "changes",
    title: "Changes to these terms",
    body: (
      <p>
        We will announce material changes on this page and by email to account holders before they
        apply. If you do not accept a change, you can stop using convt and delete your account.
      </p>
    ),
  },
  {
    id: "law",
    title: "Governing law",
    body: (
      <p>
        These terms are governed by {legal.governingLaw}, and any dispute about them goes to{" "}
        {legal.courts}. If you are a consumer, you also keep the protection of the mandatory law of
        the country where you live, and can bring a claim in its courts.
      </p>
    ),
  },
  {
    id: "contact",
    title: "Contact",
    body: (
      <p>
        Questions about these terms, refunds or your license:{" "}
        <TextLink href={`mailto:${SUPPORT_EMAIL}`}>{SUPPORT_EMAIL}</TextLink>.
      </p>
    ),
  },
];
