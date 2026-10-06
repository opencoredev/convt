# PDFium chromium/8076 source delivery

`pdfium-source.lock.json` retains the exact non-V8 source closure for the existing Linux x64 and macOS arm64/x64/universal binary pins. It includes the upstream PDFium revision, all 26 active Git DEPS, recursive build/buildtools DEPS, the Freetype dlg gitlink, the binary vendor's complete build recipe and patches, and the matching LLVM runtime sources. libc++, libc++abi and compiler-runtime notices are included separately because the vendor notice collector skips libc++.

The lock reproduces all 14 component notices from the source bytes and checks them against each binary archive. The macOS PDFium notice preserves the source's `//` prefixes; the Linux notice strips them. Both forms are accounted for. The release attestation's decoded subject digests and builder commit are checked. Its Sigstore signature and transparency proof have not been cryptographically verified by this collector.

Run from a checkout or frozen release source tree:

```sh
export CONVT_BUNDLE_CACHE=/absolute/path/to/convt/packaging/.cache
python3 packaging/release/pdfium-source-verify.py packaging/release/pdfium-source.lock.json
SOURCE_DATE_EPOCH=1791331200 python3 packaging/release/pdfium-source-collect.py
```

`--fetch` retrieves absent artifacts and refuses bytes that differ from the lock. Downloads stay in the configured cache. Collection refuses an existing output. `--output` must point inside that cache. The default output is `pdfium-source-delivery.tar.gz`.

The archive preserves every artifact's `packaging/.cache/<cache_filename>` path, including binary builder tools. It contains the lock, these scripts, the vendor's complete rebuild scripts and patches, extracted binary notices and supplemental runtime notices. The collector validates the completed archive's member hashes before returning success. Extract it, then pass its PDFium lock explicitly to the verifier; the default verifier also expects the separate macOS FFmpeg research lock.

To reconstruct the PDFium checkout, unpack the PDFium archive at the source root and each Git dependency at its locked `checkout_path`. Gitiles archives have no wrapper directory. The dlg archive requires its single wrapper directory removed. Keep separately captured LLVM runtime source outside the PDFium checkout. The upstream recipe's checkout step performs network access; for an offline rebuild, replace that step with this archive assembly and use the locked GN, Ninja, Clang and sysroot inputs. Generate gclient arguments and LASTCHANGE from the recorded revision, apply the pinned shared/public-header/clang-runtime patches and the macOS patch when appropriate, then use the vendor's configure/build steps. An offline rebuild has not been executed here.

`remaining_source_gaps` records missing source evidence. `not_checked` records rebuild and environment checks. `integration_requirements` records work for the release and macOS owners. These categories do not change any publication gate. Parent integration must include the sources and full notices in the final release archive and installed bundle.
