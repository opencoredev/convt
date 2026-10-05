---
name: test-convt-web
description: Run and verify the convt.app website (apps/web, TanStack Start on Cloudflare Workers) locally, in a browser at desktop and mobile widths. Use when changing anything under apps/web, or when asked to preview, screenshot or check the site.
---

# Test the convt website

`apps/web` is a TanStack Start app built with Vite and the Cloudflare plugin, deployed with Wrangler as the `convt-web` Worker. Today it has one placeholder route (`src/routes/index.tsx`). It has no auth, API calls, database or environment variables, so a local run needs no credentials.

## Doctor

```sh
cargo locate-project --workspace --message-format plain   # this checkout
bun install --frozen-lockfile
ss -ltnp 'sport = :3000'                                    # who owns the default port
```

If something already listens on 3000, check that it is this checkout's Vite (its command line includes this repo's `apps/web`) before reusing it. Otherwise start on another port rather than killing it.

## Launch

```sh
work=$(mktemp -d /tmp/convt-web.XXXXXX)
port=3000   # pick another if taken
(cd apps/web && exec bunx vite dev --port "$port" --strictPort) >"$work/dev.log" 2>&1 &
echo $! > "$work/dev.pid"
for _ in $(seq 60); do curl -fsS "http://127.0.0.1:$port/" 2>/dev/null | grep -q '<title>convt</title>' && break; sleep 0.5; done
curl -fsS "http://127.0.0.1:$port/" | grep -ao '<h1[^>]*>[^<]*'   # -a: inline scripts look binary to grep
```

`--strictPort` makes Vite fail instead of silently moving to another port. Readiness is HTML from `/` that contains `<title>convt</title>`. The Cloudflare plugin runs the SSR code in workerd, so a server-side error shows up in `dev.log`, not the browser console.

For a production-like check, build and run the Worker locally instead: `bun run build`, then `bunx wrangler dev` from `apps/web`. Never run `bun run deploy` or `wrangler deploy` without the user's explicit authorization.

## Drive and evidence

Opening a browser needs the user's explicit permission in the current request; a request to test or fix the site does not count. Without it, verify with `curl` and the checks below, and report rendering and layout as pending. With permission, use headless `agent-browser` with a session and profile owned by this repo, never a personal browser profile.

```sh
s=convt-web; p=~/.agent-browser/profiles/convt-web
agent-browser --session $s --profile $p open "http://127.0.0.1:$port/"
agent-browser --session $s snapshot                       # semantic tree: check headings and text
agent-browser --session $s screenshot "$work/desktop.png"
agent-browser --session $s set viewport 390 844
agent-browser --session $s screenshot "$work/mobile.png"
```

Run `agent-browser skills get core --full` if a command differs in the installed version. A layout change needs both the desktop and the 390 px mobile screenshot. Look at each one with the Read tool. When routes or links exist, follow them and check the browser console for errors; a rendered first page is not proof that navigation works.

If `agent-browser` is unavailable, fall back to `curl` for the HTML and report that rendering, layout and interaction were not verified.

## Preview for another device

The dev server binds to localhost. Sharing it, for example over Tailscale, means binding to another interface (`--host`) and verifying the URL from the device that will open it. Do that only when asked, and report which devices actually loaded the page. The site has no cookies or OAuth today, so a plain HTTP tailnet address works; that will change once auth lands.

## Checks

```sh
bun run check && bun run check-types && bun run build
```

## Cleanup

`kill "$(cat "$work/dev.pid")"`, close the browser session with `agent-browser --session convt-web close`, then `rm -rf "$work"` unless the screenshots are still needed. Leave any server you reused running.
