import { createFileRoute } from "@tanstack/react-router";

import { TextLink } from "#/components/site/layout";
import { LegalPage, List, Placeholder, type LegalSection } from "#/components/site/legal";
import { PRIVACY_EMAIL, legal, routes, seo } from "#/lib/site";

// DRAFT pending Leo's legal review. Keep it matched to what the product does:
// docs/plan.md (P8, P9, P11) and docs/document-pack.md describe every network call.

export const Route = createFileRoute("/_site/privacy")({
  head: () =>
    seo({
      title: "Privacy policy · convt",
      description:
        "What convt collects and why. The desktop app converts files on your computer and sends nothing unless you ask it to.",
      path: routes.privacy,
    }),
  component: () => (
    <LegalPage
      title="Privacy policy"
      summary={
        <p>
          convt converts files on your own computer. The app has no analytics or tracking, and your
          files stay on your machine unless you choose to convert one in the cloud. This policy
          covers the convt app, the command line tool, convt.app and the convt API.
        </p>
      }
      sections={sections}
    />
  ),
});

const strong = "font-medium text-ink";

const sections: LegalSection[] = [
  {
    id: "who",
    title: "Who we are",
    body: (
      <p>
        convt is run by <Placeholder>{legal.entity}</Placeholder>,{" "}
        <Placeholder>{legal.address}</Placeholder> ("we"). We are the controller of the personal
        data described here. Write to{" "}
        <TextLink href={`mailto:${PRIVACY_EMAIL}`}>{PRIVACY_EMAIL}</TextLink> with any question
        about it.
      </p>
    ),
  },
  {
    id: "desktop-app",
    title: "The desktop app and command line tool",
    body: (
      <>
        <p>
          Conversions run on your computer. The app and the <code>convt</code> command never upload
          a file, and contain no analytics, crash reporting or advertising code. The trial and your
          license key are stored on your computer and checked offline.
        </p>
        <p>The app connects to the internet only in these cases:</p>
        <List
          items={[
            <>
              <span className={strong}>Update check.</span> At most once a day, while update checks
              are on in Settings, the app downloads a list of available versions from convt.app. The
              request carries your IP address and app version, like any web request, and nothing
              about your files. You can turn it off.
            </>,
            <>
              <span className={strong}>Pro renewal.</span> If you have signed in to convt Pro from
              the app, it asks convt.app for your current Pro key at most once a day at launch, and
              when you click Refresh license. The request identifies your account and this computer
              through the sign-in token. Desktop license owners never sign in.
            </>,
            <>
              <span className={strong}>Things you ask for.</span> Signing in, installing the
              optional document pack, downloading an update and sending a file to the cloud each
              happen only when you click to do them.
            </>,
          ]}
        />
      </>
    ),
  },
  {
    id: "cloud",
    title: "Cloud conversions and the API",
    body: (
      <>
        <p>
          convt Pro can convert a file in the cloud, from the web converter or when you choose the
          cloud for a job in the app, which asks you each time. The API converts files your code
          sends. In these cases we receive the file and process it only to convert it for you.
        </p>
        <List
          items={[
            <>
              Input and output files are{" "}
              <span className={strong}>deleted 24 hours after the job is created</span>. We do not
              look at, keep or train anything on them.
            </>,
            "Each conversion runs in an isolated sandbox with no network access.",
            "We keep a record of each job without the file: its formats, size, status, timing and any error, for usage limits, billing and support.",
            "API keys are stored only as a hash. You see a key once, when you create it.",
          ]}
        />
      </>
    ),
  },
  {
    id: "data",
    title: "What we collect on convt.app",
    body: (
      <>
        <p>We collect only what the account, licensing and billing features need:</p>
        <List
          items={[
            <>
              <span className={strong}>Account:</span> your email address and name, and your profile
              picture if you sign in with GitHub or Google. We do not store passwords; you sign in
              with an emailed code or with GitHub or Google.
            </>,
            <>
              <span className={strong}>Sessions and security:</span> a session cookie, and the IP
              address and browser of each session, so you can see where you are signed in. We count
              sign-in attempts per email address and IP address to stop abuse.
            </>,
            <>
              <span className={strong}>Computers:</span> for each computer signed in to Pro, a name,
              its operating system and app version, so you can revoke it from the dashboard.
            </>,
            <>
              <span className={strong}>Purchases:</span> your orders, subscriptions, invoices and
              license keys. Card details go to our payment provider and never reach us.
            </>,
            <>
              <span className={strong}>Site analytics:</span> convt.app uses PostHog (US) to record
              page views and clicks on this website so we can see which pages are used. That is only
              on convt.app. The desktop app and the command line tool still send nothing.
            </>,
          ]}
        />
        <p>
          The site sets the cookies it needs to work: the sign-in session and, during a purchase, a
          short-lived cookie that lets this browser show your new license key. PostHog may also
          store a first-party identifier in this browser for those page views. There is no
          advertising or other third-party tracking.
        </p>
      </>
    ),
  },
  {
    id: "providers",
    title: "Who processes data for us",
    body: (
      <>
        <List
          items={[
            <>
              <span className={strong}>Polar</span> is the merchant of record: it sells convt to
              you, takes payment, handles sales tax and VAT and issues receipts. Polar receives your
              payment details and billing address under its own privacy policy.
            </>,
            <>
              <span className={strong}>Resend</span> delivers our emails: sign-in codes, license
              keys, receipts and account notices. We send no marketing email.
            </>,
            <>
              <span className={strong}>Cloudflare</span> hosts convt.app and stores cloud conversion
              files.
            </>,
            <>
              <span className={strong}>PostHog</span> records page views and clicks on convt.app
              under its own privacy policy.
            </>,
            <>
              <span className={strong}>Railway</span> hosts our Postgres database and the conversion
              servers.
            </>,
            <>
              <span className={strong}>GitHub and Google</span> receive a sign-in request only if
              you choose to sign in with them.
            </>,
          ]}
        />
        <p>We do not sell personal data or share it with advertisers.</p>
      </>
    ),
  },
  {
    id: "retention",
    title: "How long we keep it",
    body: (
      <List
        items={[
          "Cloud conversion files: 24 hours.",
          "Account data: until you delete your account. Delete it from Settings on the dashboard; any subscription ends first.",
          "Orders, invoices and license records: kept after you delete your account, as tax and accounting law requires. They keep the email address used for the purchase.",
          "Sign-in codes expire after 15 minutes; sessions end when you sign out or they expire.",
        ]}
      />
    ),
  },
  {
    id: "rights",
    title: "Your rights",
    body: (
      <>
        <p>
          Depending on where you live, you can ask for a copy of your data, correct it, delete it,
          object to or restrict how we use it, and take it elsewhere. Most of it is on your
          dashboard, and you can delete your account there. For anything else, write to{" "}
          <TextLink href={`mailto:${PRIVACY_EMAIL}`}>{PRIVACY_EMAIL}</TextLink>. You can also
          complain to your local data protection authority.
        </p>
        <p>
          We process account and purchase data to provide what you signed up for or bought, and
          security data for our legitimate interest in keeping the service safe.{" "}
          <Placeholder>[Legal bases and international transfer mechanism to confirm]</Placeholder>
        </p>
      </>
    ),
  },
  {
    id: "changes",
    title: "Changes",
    body: (
      <p>
        If this policy changes in a way that matters, we will say so on this page and email account
        holders before it takes effect. Older versions stay available on request.
      </p>
    ),
  },
];
