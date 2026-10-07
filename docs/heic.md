# HEIC and AVIF engine

The `libheif` engine decodes HEIC and AVIF to PNG and encodes raster inputs to HEIC or AVIF. Each route is offered only when the loaded library provides the corresponding decoder or can instantiate the encoder. Conversion runs in process, so CLI, GUI and FFI callers use the same engine.

## Library and plugin loading

Discovery checks an absolute `CONVT_LIBHEIF_DIR`, the executable directory and fixed bundle directories. Unix also searches system library names. On Windows, use the bundled library or an absolute `CONVT_LIBHEIF_DIR`; `PATH` and the working directory are not searched. Dependent DLLs must be beside the selected library or in System32. A library must export `struct heif_error heif_convt_init_no_plugins(void)`. The bundled libheif 1.17.6 patch supplies that initializer: it initializes colour conversion, built-in codecs and the library reference count without loading automatic plugins. The engine calls it before any codec or context API, then balances its reference with `heif_deinit` when dropped.

Unpatched system libraries are unavailable, even if they have working codecs. The unavailable reason names the missing secure initializer. Ordinary `heif_init` and context allocation can consume inherited plugin paths, so neither is a safe substitute.

After secure initialization, the engine calls `heif_load_plugins` for absolute `libheif/plugins` paths in the fixed bundle layout and an absolute `CONVT_LIBHEIF_PLUGIN_DIR`, if set. It ignores relative overrides, including `.`, and does not consume `LIBHEIF_PLUGIN_PATH` or change the process environment. An explicit absolute plugin override selects native code to load; use a trusted directory.

## Colour and precision

Orientation is applied before resizing or encoding. ICC profiles exposed by the image decoder are retained. Without ICC, the engine maps PNG metadata to an NCLX output profile:

- `cICP` supplies primaries and transfer characteristics and takes precedence over `sRGB`, `gAMA` and `cHRM`. PNG `cICP` must describe full-range RGB.
- `sRGB` selects sRGB primaries and transfer, overriding `gAMA` and `cHRM`.
- `cHRM` maps all primary-coordinate sets defined by libheif 1.17.6's NCLX codes: BT.709, BT.470M, BT.470BG, BT.601/SMPTE 240M, generic film, BT.2020, XYZ, DCI P3, Display P3 and EBU Tech 3213. BT.601 and SMPTE 240M share coordinates; `cHRM` alone selects code 6, while explicit `cICP` can select code 7. Other valid coordinates are converted to sRGB.
- `gAMA` maps linear, gamma 2.2 and gamma 2.8 to their NCLX transfer codes. Other positive gamma values are converted to the sRGB transfer curve while retaining primaries and alpha. Zero gamma returns an error.

For custom `cHRM`, conversion derives an RGB-to-XYZ matrix from the primaries and white point, adapts that white to D65 with the Bradford transform, then converts to sRGB. It decodes the source gamma or sRGB transfer before matrix multiplication and applies the sRGB transfer afterward. Out-of-gamut RGB values are clipped; alpha is retained. Invalid coordinates, degenerate matrices or an invalid white point return an error. Some codec implementations reject particular otherwise valid NCLX codes: bundled x265 rejects EBU code 22, which is tested through AVIF instead.

Other decoder colour-space metadata uses the image buffer's CICP primaries and transfer. Untagged inputs default to sRGB. The output profile uses full-range BT.601 YCbCr matrix coefficients for the codec's RGB-to-YCbCr conversion. Encoding options explicitly supply NCLX and enable its output, including for HEIC on libheif 1.17.6.

Sixteen-bit and floating-point inputs use 10-bit samples for encoding. HEIC first attempts 10-bit output; only an explicit `heif_suberror_Unsupported_bit_depth` triggers a fresh 8-bit attempt. The bundled x265 build currently takes this fallback. A HEVC encoder with 10-bit support produces 10-bit HEIC. AVIF uses 10-bit output and reports an encoding error if that depth is unsupported. Eight-bit inputs remain 8-bit unless colour conversion needs greater precision. HEIC/AVIF encoding is lossy; this is not preservation of all 16 source bits.

## Verification

Run `cargo test -p convt-engines --lib heic::tests -- --test-threads=1 --nocapture` with `CONVT_LIBHEIF_DIR` pointing at the patched library and an absolute `CONVT_LIBHEIF_PLUGIN_DIR` pointing at its codecs. Use `LD_LIBRARY_PATH` on Linux when codec dependencies are outside the system search path. Set `CONVT_TEST_REQUIRE_HEIF=1` to make the colour and bit-depth regressions fail rather than skip when their encoders are missing.

The tests inspect all mapped NCLX primary codes, check custom Adobe RGB conversion against reference pixels and white-point adaptation, inspect colour-channel bit depths, round-trip alpha, dimensions and orientation, and attempt HEIC at 10 bits before checking either 10-bit output or the explicit 8-bit fallback. `CONVT_TEST_HEIC_DEPTH=8` or `10` requires the selected HEVC encoder to exercise that branch. Constructor-sentinel tests use subprocess environments to check absolute and relative inherited plugin paths, a relative explicit override, and rejection of an unpatched library. The ignored `worker_entry` is only a test harness entry that constructs an engine; production conversion has no worker subprocess.

The existing CLI matrix checks decoded colour against generated fixtures. Select `CONVT_MATRIX_INPUT=png` and `CONVT_MATRIX_TARGET=heic,avif`, set `CONVT_BIN` to the CLI being tested, and select the bundled `CONVT_FFMPEG` and `CONVT_FFPROBE`. The separate Python validator still uses upstream libheif initialization and needs its own trusted `LIBHEIF_PLUGIN_PATH` when validating codecs outside their compiled installation path. This validator setting does not change the engine's plugin discovery.
