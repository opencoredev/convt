# convt browser extension

A Chrome and Edge extension that adds **Convert with convt** to the right-click menu on images: Save as PNG, Save as JPG, Save as WebP, and Copy as PNG. It converts inside the browser with the canvas encoder. The only network request it makes is fetching the image the user right-clicked.

It is also a way into the desktop app. When a conversion runs into something only the app can do (HEIC, animated GIF to MP4, images too large for the browser), the toast says so and links to convt.app. Every link carries `utm_source=chrome-extension`, `utm_medium=extension`, `utm_campaign=image-converter` and a `utm_content` naming the spot it came from, so PostHog on the site can attribute visits. The extension itself has no analytics.

## Develop

```sh
bun run --cwd apps/extension build      # dist/ plus convt-extension-<version>.zip
bun run --cwd apps/extension dev        # rebuild dist/ on every change
bun run --cwd apps/extension test       # unit tests
bun run --cwd apps/extension test:e2e   # headless Chrome end to end (needs agent-browser and ffmpeg)
bun run --cwd apps/extension check-types
```

To try it, open `chrome://extensions`, turn on Developer mode, choose **Load unpacked** and pick `apps/extension/dist`. The welcome page opens with a photo to right-click. After a rebuild, press the reload arrow on the extension's card.

The end-to-end test builds `dist-e2e/`, which differs from the release build in three ways: the background worker accepts a `test:run` message (headless Chrome can't click native menus, so the test sends the same job a menu click would), the toast logs its states where the test can read them, and the manifest adds `clipboardRead` and access to `localhost`. `dist-e2e/gallery.html` renders every toast state for screenshots. Set `KEEP=1` to leave the browser running after the test, and stop it with Ctrl-C.

Chrome's site-access dialog can't be clicked headless, so the test covers the access flow up to that dialog and the resume after it, but not the dialog itself. Check that by hand: load the unpacked build, right-click an image on a site that serves images from a CDN without CORS, choose **Allow access**, and accept Chrome's prompt.

## How a conversion runs

1. `src/background/index.ts` gets the menu click and injects `toast.js` into the page (allowed by `activeTab` after the right-click).
2. `src/background/acquire.ts` gets the bytes: `data:` URLs directly; otherwise the worker fetches, then the page fetches (it has the page's cookies, and `blob:` URLs only resolve there). When both are refused and convt has no access to the image's host, the job waits in session storage and the toast offers **Allow access**.
3. `src/offscreen/transcode.ts` decodes and re-encodes in an offscreen document, which has the DOM that SVG decoding needs. JPG gets a white background. Animated sources keep their first frame.
4. The file downloads through `chrome.downloads` with a name from `src/shared/naming.ts`. A download listener finishes the toast, so a worker suspended during a Save As dialog still reports the result.

All user-facing toast text is in `src/shared/copy.ts`.

## Permissions

| Permission               | Why                                                                                                                               | Install warning                       |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| `contextMenus`           | The right-click menu                                                                                                              | none                                  |
| `downloads`              | Save the converted file and show it in its folder                                                                                 | "Manage your downloads"               |
| `clipboardWrite`         | Copy as PNG. A menu click gives the page no user activation, so the page's own clipboard access is refused without it.            | "Modify data you copy and paste"      |
| `activeTab`, `scripting` | Show the toast and fetch the image in the page the user right-clicked, only after the right-click                                 | none                                  |
| `storage`                | Settings, recent files, jobs waiting for access                                                                                   | none                                  |
| `offscreen`              | Decode and encode images with the DOM                                                                                             | none                                  |
| `<all_urls>` (optional)  | Read images from hosts that don't allow cross-origin reads. Asked for only when a conversion needs it, per host or for all sites. | Shown in Chrome's prompt at that time |

## Store listing

`store/listing.md` has every field the Chrome Web Store dashboard asks for, in dashboard order, including the permission justifications. The screenshots and promo tile in `store/` come from the real extension: `bun run --cwd apps/extension store-shots` regenerates them. Edge Add-ons takes the same zip and text.

## Not covered yet

- Firefox: it has no offscreen documents, so the converter would run in the background page instead.
- AVIF output: Chrome's canvas can't encode AVIF.
- Edge Add-ons: needs a Partner Center account.
