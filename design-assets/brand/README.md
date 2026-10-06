# convt brand

The mark is two overlapping rounded squares: the file you have (ink) and the file you get (green). The overlap, in a lighter green, is where the conversion happens.

| File                                        | Use                                                                              |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| `mark-on-dark.svg`, `mark-on-light.svg`     | The mark alone, for dark and light backgrounds.                                  |
| `lockup-on-dark.png`, `lockup-on-light.png` | Mark plus the "convt" wordmark, 2400 px wide on a transparent background.        |
| `../../packaging/linux/convt.svg`           | The app icon (the mark on a dark tile). Linux and macOS packaging build from it. |
| `../../apps/web/public/favicon.svg`         | The favicon. It switches colors for light and dark browser tabs.                 |

The wordmark is "convt" in lowercase Geist SemiBold with -0.02em tracking. Beside it, the mark is about 1.2 times the text's font size, with a gap of half the font size.

## Colors

| Role                            | On dark                | On light               |
| ------------------------------- | ---------------------- | ---------------------- |
| Source square (ink)             | `#edefee`              | `#0a0a0a`              |
| Converted square, top to bottom | `#46d08b` to `#1fa463` | `#1fb36c` to `#127a47` |
| Overlap                         | `#a6f0c8`              | `#0b5c34`              |
| Page                            | `#0a0b0b`              | `#ffffff`              |

The site defines these as `--mark-*` tokens in `apps/web/src/styles.css`; `apps/web/src/components/logo.tsx` draws the mark from them.
