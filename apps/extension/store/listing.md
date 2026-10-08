# Chrome Web Store listing

What to enter in the [developer dashboard](https://chrome.google.com/webstore/devconsole), tab by tab. The package is `convt-extension-<version>.zip` from `bun run --cwd apps/extension build`. The images in this folder come from `bun run --cwd apps/extension store-shots`.

## Package

Upload `convt-extension-<version>.zip`. The name, summary, version and icons come from its manifest:

- **Name:** convt: Save Image as PNG, JPG or WebP
- **Summary:** Right-click any image to save it as PNG, JPG or WebP, or copy it as PNG. Converts in your browser; nothing is uploaded.

## Store listing

**Description:**

Don't list format names in the description. The store rejected 0.1.0 for excessive keywords over a bullet that listed eight formats.

```text
Right-click any image on the web and save it as PNG, JPG or WebP, or copy it as PNG.

Got a .webp you can't open? A .avif your editor doesn't know? Right-click it, choose Convert with convt, and pick the format you need. The file lands in your Downloads folder with a sensible name, and a small note in the corner of the page tells you what was saved.

• Copy as PNG to paste the image straight into a document or chat
• Converts inside your browser. Your images are never uploaded.
• Works with the images Chrome shows, from SVG icons to animated GIFs (it saves the first frame)
• Transparent images get a clean white background as JPG
• Your recent files and quality settings live in the toolbar

No account, no tracking, no ads.

Need video, audio, documents or HEIC photos? convt for desktop puts the same right-click on every file on your Mac or PC: https://convt.app
```

- **Category:** Tools
- **Language:** English
- **Store icon:** `public/icons/icon-128.png`
- **Screenshots (1280x800), in order:** `screenshot-1-right-click.png`, `screenshot-2-recent-files.png`, `screenshot-3-private.png`, `screenshot-4-welcome.png`
- **Small promo tile (440x280):** `promo-small-440x280.png`
- **Homepage URL:** https://convt.app
- **Support URL:** https://convt.app/contact

## Privacy

**Single purpose:**

```text
convt converts images on web pages to another image format (PNG, JPG or WebP) when the user right-clicks an image and picks a format, and saves the result to the user's computer or clipboard.
```

**Permission justifications:**

| Permission                               | Justification                                                                                                                                                                                                                                    |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `contextMenus`                           | Adds "Convert with convt" and its format choices to the right-click menu on images. This menu is the extension's only way to start a conversion.                                                                                                 |
| `downloads`                              | Saves the converted image to the user's Downloads folder, and opens the folder when the user clicks "Show in folder".                                                                                                                            |
| `storage`                                | Keeps the user's settings (quality, ask where to save) and a list of their last eight conversions, both in the browser.                                                                                                                          |
| `scripting`                              | After the user right-clicks an image, shows a small status note on that page and, when the page alone can read the image (cookies or blob: URLs), reads it there. Runs only on the tab the user right-clicked.                                   |
| `activeTab`                              | Gives the extension access to the page only after the user right-clicks an image on it, so it can read that image and show the status note.                                                                                                      |
| `offscreen`                              | Decodes and re-encodes the image in an offscreen document, because the background service worker has no DOM to draw images (SVG in particular).                                                                                                  |
| `clipboardWrite`                         | Copy as PNG puts the converted image on the clipboard.                                                                                                                                                                                           |
| Host permission (optional, `<all_urls>`) | Some sites only let extensions read their images with permission. The extension asks for it only when a conversion needs it, for that one site or for all sites, as the user chooses. It is used only to download images the user right-clicked. |

**Remote code:** No, I am not using remote code. All code is in the package.

**Data usage:** check none of the data types. The extension collects no user data.

Certify all three statements:

- I do not sell or transfer user data to third parties, outside of the approved use cases.
- I do not use or transfer user data for purposes that are unrelated to my item's single purpose.
- I do not use or transfer user data to determine creditworthiness or for lending purposes.

**Privacy policy URL:** https://convt.app/privacy (its "The browser extension" section)

## Distribution

- **Payments:** Free of charge
- **Visibility:** Public
- **Regions:** All regions

## Test instructions (for reviewers)

```text
No account or setup is needed. After installing, a welcome page opens with a photo: right-click it, choose Convert with convt, then Save as PNG. The PNG appears in Downloads and the result shows on the page. On any website, right-click an image and pick a format the same way; a note in the bottom-right corner shows the saved file.
```
