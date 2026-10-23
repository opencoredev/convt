import { cx, focusRing } from "#/components/app/ui";
import { otherOpenCoreSites } from "#/lib/opencore-sites";
import { SITE_ORIGIN } from "#/lib/site";

// Plain followed links on purpose: they share authority between our sites.
const sites = otherOpenCoreSites(SITE_ORIGIN);

/**
 * The footer row linking the other OpenCore sites, shared by the landing page and
 * the other public pages. `mutedClassName` is the text colour each footer gives
 * its column headings, used here for the heading and the descriptions.
 */
export function OpenCoreLinks({ mutedClassName }: { mutedClassName: string }) {
  return (
    <nav aria-labelledby="footer-opencore" className="flex flex-col gap-3">
      <h2 id="footer-opencore" className={cx("text-[13px]/5 font-medium", mutedClassName)}>
        More from OpenCore
      </h2>
      <ul className="grid gap-x-8 gap-y-3 sm:grid-cols-2 lg:grid-cols-3">
        {sites.map((site) => (
          <li key={site.url}>
            <a
              href={site.url}
              className={cx(
                "group rounded-sm text-[14px]/[18px] transition-colors",
                mutedClassName,
                focusRing,
              )}
            >
              <span className="text-ink transition-colors group-hover:text-ink-2">{site.name}</span>{" "}
              {site.description}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
