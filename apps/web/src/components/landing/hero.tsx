import { downloadEntryHref } from "#/lib/sign-in";
import { GITHUB_URL, LAUNCHED } from "#/lib/site";

import { ConvertDemo } from "./convert-demo";
import { ProductHuntBadge } from "./product-hunt-badge";
import { ButtonLink, ComingSoon, Container, DownloadIcon } from "./ui";

export function Hero({ signedIn }: { signedIn: boolean }) {
  return (
    <Container>
      <div className="flex flex-col items-center gap-[22px] pt-14 pb-12 text-center md:pt-[88px] md:pb-16">
        <h1 className="max-w-[900px] text-[40px]/[44px] font-medium tracking-[-0.04em] text-balance text-ink sm:text-[56px]/[60px] lg:text-[68px]/[72px]">
          Convert any file
          <br className="hidden sm:inline" /> with a right-click.
        </h1>
        <p className="max-w-[560px] text-[17px]/[26px] text-ink-2 sm:text-[18px]/[28px]">
          Images, video, audio and documents, converted on your own computer. Nothing gets uploaded.
        </p>
        <div className="flex flex-wrap justify-center gap-2.5 pt-2.5">
          {LAUNCHED ? (
            <HeroActions signedIn={signedIn} />
          ) : (
            <ComingSoon className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]">
              Not available yet
            </ComingSoon>
          )}
        </div>
        <ProductHuntBadge />
      </div>
      <ConvertDemo />
    </Container>
  );
}

// "Get convt" goes to /download, which asks a new visitor to create an account first, so a
// signed-out visitor goes straight to sign-in.
function HeroActions({ signedIn }: { signedIn: boolean }) {
  return (
    <>
      <ButtonLink
        variant="primary"
        href={downloadEntryHref(signedIn)}
        className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
      >
        <DownloadIcon />
        Get convt
      </ButtonLink>
      <ButtonLink
        variant="secondary"
        href={GITHUB_URL}
        className="rounded-[10px] px-[18px] py-[11px] text-[15px]/[18px]"
      >
        Star on GitHub
      </ButtonLink>
    </>
  );
}
