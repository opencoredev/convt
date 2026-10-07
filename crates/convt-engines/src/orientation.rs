//! Display orientation for still images.
//!
//! Preview shows pixels after EXIF orientation and, for HEIF, the `irot` /
//! `imir` item properties. Those tags are then discarded. If we strip the tag
//! without applying it, a WebP (which cannot keep EXIF the way we encode it)
//! comes out on its side. If we apply the tag and also leave it in the file,
//! the next hop rotates the picture again.

use std::path::Path;

#[cfg(target_os = "macos")]
use convt_core::Options;
use convt_core::Result;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};

#[cfg(target_os = "macos")]
use crate::image::encode;
use crate::image::failed;

/// Decodes `input` the way Preview would show it: pixels after EXIF
/// orientation, with the tag consumed so a later encode cannot apply it twice.
pub(crate) fn open_oriented(input: &Path) -> Result<DynamicImage> {
    let mut decoder = ImageReader::open(input)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(failed)?;
    let orientation = decoder.orientation().map_err(failed)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(failed)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// The EXIF orientation the decoder reports, or `NoTransforms` when the
/// format has no tag. Does not apply it.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn decoder_orientation(input: &Path) -> Result<Orientation> {
    let mut decoder = ImageReader::open(input)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(failed)?;
    decoder.orientation().map_err(failed)
}

/// Reads pixels without applying EXIF, for comparing baked output against a
/// leftover tag that would rotate the picture a second time.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn open_stored(input: &Path) -> Result<DynamicImage> {
    ImageReader::open(input)?
        .with_guessed_format()?
        .decode()
        .map_err(failed)
}

/// Rewrites `path` with EXIF orientation baked in and the tag gone.
#[cfg(target_os = "macos")]
pub(crate) fn bake_pending_orientation(path: &Path, to: &str, options: &Options) -> Result<()> {
    if decoder_orientation(path)? == Orientation::NoTransforms {
        return Ok(());
    }
    let img = open_oriented(path)?;
    encode(img, to, options, path)
}

/// After a tool such as `sips` writes `dest` from `source`, make the pixels
/// match Preview. ImageIO sometimes applies HEIF transforms, sometimes copies
/// EXIF, and sometimes drops both. Stored size (`ispe`) versus output size
/// tells us whether a 90° transform already ran; leftover EXIF on the output
/// is always baked in.
#[cfg(target_os = "macos")]
pub(crate) fn finish_heif_output(
    source: &Path,
    dest: &Path,
    to: &str,
    options: &Options,
) -> Result<()> {
    bake_pending_orientation(dest, to, options)?;
    let Some(file) = std::fs::read(source).ok() else {
        return Ok(());
    };
    let Some(heif) = heif_display_orientation(&file) else {
        if decoder_orientation(dest)? == Orientation::NoTransforms
            && let Some(exif) = heif_exif_orientation(&file)
        {
            apply_if_still_stored(dest, to, options, exif, heif_ispe(&file))?;
        }
        return Ok(());
    };
    if heif == Orientation::NoTransforms {
        return Ok(());
    }
    apply_if_still_stored(dest, to, options, heif, heif_ispe(&file))
}

#[cfg(target_os = "macos")]
fn apply_if_still_stored(
    dest: &Path,
    to: &str,
    options: &Options,
    orientation: Orientation,
    stored: Option<(u32, u32)>,
) -> Result<()> {
    let img = open_stored(dest)?;
    let (w, h) = (img.width(), img.height());
    if !needs_source_orientation(orientation, (w, h), stored) {
        return Ok(());
    }
    let mut img = img;
    img.apply_orientation(orientation);
    encode(img, to, options, dest)
}

#[cfg(target_os = "macos")]
fn needs_source_orientation(
    orientation: Orientation,
    dest: (u32, u32),
    stored: Option<(u32, u32)>,
) -> bool {
    let display = display_size(orientation, stored.unwrap_or(dest));
    if swaps_dims(orientation) {
        dest != display
    } else {
        // 180° and mirrors do not change size. If the writer already baked
        // the pixels and stripped the tag, applying again would flip twice.
        // Only apply when the output is still the stored size *and* we have
        // no other signal — which we treat as "writer dropped the tag".
        // Callers only reach this for files that still look untransformed
        // (no dest EXIF). Prefer not to guess: leave same-size transforms
        // to leftover dest EXIF, which `bake_pending_orientation` handled.
        false
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn swaps_dims(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn display_size(orientation: Orientation, stored: (u32, u32)) -> (u32, u32) {
    if swaps_dims(orientation) {
        (stored.1, stored.0)
    } else {
        stored
    }
}

/// EXIF orientation inside a JPEG APP1 payload, a raw TIFF, or a HEIF Exif
/// item (4-byte offset prefix, optional `Exif\0\0`).
pub(crate) fn orientation_from_exif_bytes(data: &[u8]) -> Option<Orientation> {
    if let Some(ori) = Orientation::from_exif_chunk(data) {
        return Some(ori);
    }
    if data.len() >= 6 && data.starts_with(b"Exif\0\0") {
        return Orientation::from_exif_chunk(&data[6..]);
    }
    if data.len() >= 8 {
        let offset = u32::from_be_bytes(data[..4].try_into().ok()?) as usize;
        for start in [4, 0, 4usize.saturating_add(offset), offset] {
            if start < data.len() {
                if let Some(ori) = Orientation::from_exif_chunk(&data[start..]) {
                    return Some(ori);
                }
                if data[start..].starts_with(b"Exif\0\0")
                    && start + 6 < data.len()
                    && let Some(ori) = Orientation::from_exif_chunk(&data[start + 6..])
                {
                    return Some(ori);
                }
            }
        }
    }
    None
}

/// `irot` / `imir` on the primary image, mapped to the equivalent EXIF
/// orientation. `None` when the file has no HEIF transforms — EXIF may
/// still apply.
pub(crate) fn heif_display_orientation(data: &[u8]) -> Option<Orientation> {
    let (irot, imir) = heif_transforms(data)?;
    match heif_transforms_to_orientation(irot.unwrap_or(0), imir) {
        Orientation::NoTransforms => None,
        other => Some(other),
    }
}

pub(crate) fn heif_transforms_to_orientation(irot_angle: u8, imir: Option<u8>) -> Orientation {
    // HEIF applies irot (90° CCW steps) then imir. Image crate orientations
    // rotate clockwise first, then flip horizontally for the 5/7 cases.
    match (irot_angle & 3, imir) {
        (0, None) => Orientation::NoTransforms,
        (0, Some(1)) => Orientation::FlipHorizontal,
        (0, Some(_)) => Orientation::FlipVertical,
        (1, None) => Orientation::Rotate270,
        (1, Some(1)) => Orientation::Rotate270FlipH,
        (1, Some(_)) => Orientation::Rotate90FlipH,
        (2, None) => Orientation::Rotate180,
        (2, Some(1)) => Orientation::FlipVertical,
        (2, Some(_)) => Orientation::FlipHorizontal,
        (3, None) => Orientation::Rotate90,
        (3, Some(1)) => Orientation::Rotate90FlipH,
        (3, Some(_)) => Orientation::Rotate270FlipH,
        _ => Orientation::NoTransforms,
    }
}

fn heif_transforms(data: &[u8]) -> Option<(Option<u8>, Option<u8>)> {
    let ipco = find_box_payload(data, b"ipco")?;
    let mut irot = None;
    let mut imir = None;
    for_each_box(ipco, |typ, payload| {
        if typ == *b"irot" && !payload.is_empty() {
            irot = Some(payload[0] & 3);
        }
        if typ == *b"imir" && !payload.is_empty() {
            imir = Some(payload[0] & 1);
        }
    });
    if irot.is_none() && imir.is_none() {
        None
    } else {
        Some((irot, imir))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn heif_ispe(data: &[u8]) -> Option<(u32, u32)> {
    let ipco = find_box_payload(data, b"ipco")?;
    let mut size = None;
    for_each_box(ipco, |typ, payload| {
        if typ == *b"ispe" && payload.len() >= 12 {
            let width = u32::from_be_bytes(payload[4..8].try_into().unwrap());
            let height = u32::from_be_bytes(payload[8..12].try_into().unwrap());
            if width > 0 && height > 0 {
                size = Some((width, height));
            }
        }
    });
    size
}

/// EXIF orientation stored as a HEIF `Exif` item, if any.
pub(crate) fn heif_exif_orientation(data: &[u8]) -> Option<Orientation> {
    let mut found = None;
    for_each_box(data, |typ, payload| {
        if found.is_some() {
            return;
        }
        if typ == *b"meta" && payload.len() > 4 {
            found = heif_exif_in_meta(&payload[4..], data);
        }
    });
    found
}

fn heif_exif_in_meta(meta: &[u8], file: &[u8]) -> Option<Orientation> {
    let mut exif_id = None;
    let mut locations: Vec<(u32, u64, u64)> = Vec::new();
    for_each_box(meta, |typ, payload| {
        if typ == *b"iinf" && payload.len() > 4 {
            exif_id = exif_id.or_else(|| exif_item_id(&payload[4..]));
        }
        if typ == *b"iloc" {
            locations = parse_iloc(payload);
        }
    });
    let id = exif_id?;
    let (_, offset, length) = locations.into_iter().find(|(item, _, _)| *item == id)?;
    let start = offset as usize;
    let end = start.saturating_add(length as usize);
    if end > file.len() || start >= file.len() {
        return None;
    }
    orientation_from_exif_bytes(&file[start..end])
}

fn exif_item_id(iinf: &[u8]) -> Option<u32> {
    // FullBox version/flags already stripped by the caller of iinf's payload
    // after its own 4-byte header. `iinf` payload here still includes that
    // header when we passed payload[4..] from the box walker — `iinf` is a
    // FullBox, so `payload` starts with version/flags. The caller stripped
    // those 4 bytes; remaining is entry_count then `infe` boxes.
    if iinf.len() < 2 {
        return None;
    }
    // We don't have iinf version here (stripped). entry_count is 16-bit in
    // version 0 and 32-bit in version 1. Try both by scanning infe boxes.
    let mut rest = if iinf.len() >= 4 && u32::from_be_bytes(iinf[..4].try_into().ok()?) < 256 {
        &iinf[4..]
    } else {
        &iinf[2..]
    };
    while rest.len() >= 8 {
        let size = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        if size < 8 || size > rest.len() {
            break;
        }
        if &rest[4..8] == b"infe" {
            let body = &rest[8..size];
            // FullBox + item_ID + protection + type ('Exif')
            if body.len() >= 12 {
                let version = body[0];
                let (id, type_at) = if version >= 3 {
                    if body.len() < 14 {
                        rest = &rest[size..];
                        continue;
                    }
                    (u32::from_be_bytes(body[4..8].try_into().ok()?), 10)
                } else if version >= 2 {
                    (u16::from_be_bytes(body[4..6].try_into().ok()?) as u32, 8)
                } else {
                    rest = &rest[size..];
                    continue;
                };
                if type_at + 4 <= body.len() && &body[type_at..type_at + 4] == b"Exif" {
                    return Some(id);
                }
            }
        }
        rest = &rest[size..];
    }
    None
}

fn parse_iloc(payload: &[u8]) -> Vec<(u32, u64, u64)> {
    // payload includes FullBox version/flags.
    if payload.len() < 8 {
        return Vec::new();
    }
    let version = payload[0];
    let sizes = payload[4];
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0xf) as usize;
    let base_offset_size = (payload[5] >> 4) as usize;
    let index_size = if version == 1 || version == 2 {
        (payload[5] & 0xf) as usize
    } else {
        0
    };
    let (mut i, item_count) = if version < 2 {
        if payload.len() < 8 {
            return Vec::new();
        }
        (
            8,
            u16::from_be_bytes(payload[6..8].try_into().unwrap()) as u32,
        )
    } else {
        if payload.len() < 10 {
            return Vec::new();
        }
        (10, u32::from_be_bytes(payload[6..10].try_into().unwrap()))
    };
    let mut out = Vec::new();
    for _ in 0..item_count {
        let id = if version < 2 {
            if i + 2 > payload.len() {
                break;
            }
            let id = u16::from_be_bytes(payload[i..i + 2].try_into().unwrap()) as u32;
            i += 2;
            id
        } else {
            if i + 4 > payload.len() {
                break;
            }
            let id = u32::from_be_bytes(payload[i..i + 4].try_into().unwrap());
            i += 4;
            id
        };
        if version == 1 || version == 2 {
            i += 2; // construction_method + reserved
        }
        i += 2; // data_reference_index
        if i + base_offset_size > payload.len() {
            break;
        }
        let base = read_size(&payload[i..], base_offset_size);
        i += base_offset_size;
        if i + 2 > payload.len() {
            break;
        }
        let extents = u16::from_be_bytes(payload[i..i + 2].try_into().unwrap());
        i += 2;
        for _ in 0..extents {
            i += index_size;
            if i + offset_size + length_size > payload.len() {
                return out;
            }
            let offset = base + read_size(&payload[i..], offset_size);
            i += offset_size;
            let length = read_size(&payload[i..], length_size);
            i += length_size;
            out.push((id, offset, length));
        }
    }
    out
}

fn read_size(data: &[u8], size: usize) -> u64 {
    match size {
        0 => 0,
        4 if data.len() >= 4 => u32::from_be_bytes(data[..4].try_into().unwrap()).into(),
        8 if data.len() >= 8 => u64::from_be_bytes(data[..8].try_into().unwrap()),
        _ => 0,
    }
}

fn find_box_payload<'a>(data: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    let mut found = None;
    walk_nested(data, true, &mut |typ, payload| {
        if found.is_none() && typ == *name {
            found = Some(payload);
        }
    });
    found
}

fn for_each_box(data: &[u8], mut visit: impl FnMut([u8; 4], &[u8])) {
    walk_nested(data, false, &mut |typ, payload| visit(typ, payload));
}

fn walk_nested<'a>(data: &'a [u8], recurse: bool, visit: &mut dyn FnMut([u8; 4], &'a [u8])) {
    let mut i = 0;
    while i + 8 <= data.len() {
        let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let typ: [u8; 4] = data[i + 4..i + 8].try_into().unwrap();
        let (header, end) = if size == 1 {
            if i + 16 > data.len() {
                break;
            }
            let large = u64::from_be_bytes(data[i + 8..i + 16].try_into().unwrap()) as usize;
            (16, i.saturating_add(large))
        } else if size == 0 {
            (8, data.len())
        } else {
            (8, i.saturating_add(size))
        };
        if end > data.len() || end < i + header {
            break;
        }
        let payload = &data[i + header..end];
        visit(typ, payload);
        if recurse && matches!(&typ, b"meta" | b"iprp" | b"moov") {
            let inner = if typ == *b"meta" && payload.len() >= 4 {
                &payload[4..]
            } else {
                payload
            };
            walk_nested(inner, true, visit);
        }
        if size == 0 {
            break;
        }
        i = end;
    }
}

/// A JPEG APP1 Exif segment for one SHORT Orientation tag. Used by tests and
/// by HEIC metadata injection.
#[cfg(test)]
pub(crate) fn jpeg_exif_app1(orientation: u8) -> Vec<u8> {
    let tiff = tiff_orientation(orientation);
    let mut app1 = vec![0xff, 0xe1];
    let len = (tiff.len() + 8) as u16; // 2 length + Exif\0\0 + TIFF
    app1.extend_from_slice(&len.to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);
    app1
}

#[cfg(test)]
pub(crate) fn tiff_orientation(orientation: u8) -> Vec<u8> {
    let mut tiff = Vec::from(*b"MM\0*\0\0\0\x08\0\x01");
    tiff.extend_from_slice(&[0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01]);
    tiff.extend_from_slice(&[0x00, orientation, 0x00, 0x00]);
    tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    tiff
}

/// Appends `irot` / `imir` to the primary image's `ipco` and points `ipma`
/// at them. Media offsets in `iloc` move with the grown `meta` box.
#[cfg(test)]
pub(crate) fn inject_heif_transforms(
    data: &[u8],
    irot: Option<u8>,
    imir: Option<u8>,
) -> Result<Vec<u8>> {
    if irot.is_none() && imir.is_none() {
        return Ok(data.to_vec());
    }
    let mut out = data.to_vec();
    let mut irot = irot;
    let mut imir = imir;
    if let Some(ipco) = find_located(&out, b"ipco") {
        let mut patches = Vec::new();
        walk_located(
            &out[ipco.payload..ipco.end],
            ipco.payload,
            false,
            &mut |loc, typ| {
                if typ == *b"irot"
                    && loc.payload < loc.end
                    && let Some(angle) = irot.take()
                {
                    patches.push((loc.payload, angle & 3));
                }
                if typ == *b"imir"
                    && loc.payload < loc.end
                    && let Some(axis) = imir.take()
                {
                    patches.push((loc.payload, axis & 1));
                }
            },
        );
        for (at, val) in patches {
            out[at] = val;
        }
    }
    if irot.is_none() && imir.is_none() {
        return Ok(out);
    }
    let data = out;
    let mut extra = Vec::new();
    if let Some(angle) = irot {
        extra.extend_from_slice(&heif_box(b"irot", &[angle & 3]));
    }
    if let Some(axis) = imir {
        extra.extend_from_slice(&heif_box(b"imir", &[axis & 1]));
    }
    let ipco = find_located(&data, b"ipco").ok_or_else(|| failed("HEIF file has no ipco box"))?;
    let ipma = find_located(&data, b"ipma").ok_or_else(|| failed("HEIF file has no ipma box"))?;
    let iprp = find_located(&data, b"iprp").ok_or_else(|| failed("HEIF file has no iprp box"))?;
    let meta = find_located(&data, b"meta").ok_or_else(|| failed("HEIF file has no meta box"))?;
    if ipma.offset < ipco.end {
        return Err(failed("ipma precedes ipco"));
    }
    let existing = count_child_boxes(&data[ipco.payload..ipco.end]);
    let added = usize::from(irot.is_some()) + usize::from(imir.is_some());
    let new_ipma = add_ipma_properties(&data[ipma.payload..ipma.end], existing, added)?;
    let insert_at = ipco.end;
    let mut out = data.to_vec();
    out.splice(insert_at..insert_at, extra.iter().copied());
    add_to_size(&mut out, ipco.offset, extra.len() as i64)?;
    add_to_size(&mut out, iprp.offset, extra.len() as i64)?;
    add_to_size(&mut out, meta.offset, extra.len() as i64)?;
    let ipma_payload = ipma.payload + extra.len();
    let ipma_end = ipma.end + extra.len();
    let ipma_delta = new_ipma.len() as i64 - (ipma_end - ipma_payload) as i64;
    out.splice(ipma_payload..ipma_end, new_ipma);
    add_to_size(&mut out, ipma.offset + extra.len(), ipma_delta)?;
    add_to_size(&mut out, iprp.offset, ipma_delta)?;
    add_to_size(&mut out, meta.offset, ipma_delta)?;
    bump_iloc_offsets(&mut out, insert_at, extra.len() as i64 + ipma_delta)?;
    Ok(out)
}

/// Drops `irot` / `imir` from `ipco` and their `ipma` associations so a
/// later inject does not stack a second transform on an identity `irot`
/// that ImageIO often writes.
#[cfg(test)]
pub(crate) fn strip_heif_transforms(data: &[u8]) -> Result<Vec<u8>> {
    let Some(ipco) = find_located(data, b"ipco") else {
        return Ok(data.to_vec());
    };
    let Some(ipma) = find_located(data, b"ipma") else {
        return Ok(data.to_vec());
    };
    let Some(iprp) = find_located(data, b"iprp") else {
        return Ok(data.to_vec());
    };
    let Some(meta) = find_located(data, b"meta") else {
        return Ok(data.to_vec());
    };

    let mut keep = Vec::new();
    let mut removed = Vec::new();
    let mut index = 0usize;
    walk_located(
        &data[ipco.payload..ipco.end],
        ipco.payload,
        false,
        &mut |loc, typ| {
            index += 1;
            if typ == *b"irot" || typ == *b"imir" {
                removed.push(index);
            } else {
                keep.extend_from_slice(&data[loc.offset..loc.end]);
            }
        },
    );
    if removed.is_empty() {
        return Ok(data.to_vec());
    }
    if ipma.offset < ipco.end {
        return Err(failed("ipma precedes ipco"));
    }

    let new_ipma = strip_ipma_properties(&data[ipma.payload..ipma.end], &removed)?;
    let mut out = data.to_vec();
    let ipco_delta = keep.len() as i64 - (ipco.end - ipco.payload) as i64;
    out.splice(ipco.payload..ipco.end, keep);
    add_to_size(&mut out, ipco.offset, ipco_delta)?;
    add_to_size(&mut out, iprp.offset, ipco_delta)?;
    add_to_size(&mut out, meta.offset, ipco_delta)?;

    let ipma_payload = (ipma.payload as i64 + ipco_delta) as usize;
    let ipma_end = (ipma.end as i64 + ipco_delta) as usize;
    let ipma_offset = (ipma.offset as i64 + ipco_delta) as usize;
    let ipma_delta = new_ipma.len() as i64 - (ipma_end - ipma_payload) as i64;
    out.splice(ipma_payload..ipma_end, new_ipma);
    add_to_size(&mut out, ipma_offset, ipma_delta)?;
    add_to_size(
        &mut out,
        (iprp.offset as i64 + ipco_delta) as usize,
        ipma_delta,
    )?;
    add_to_size(&mut out, meta.offset, ipma_delta)?;
    bump_iloc_offsets(&mut out, ipco.payload, ipco_delta + ipma_delta)?;
    Ok(out)
}

#[cfg(test)]
fn strip_ipma_properties(payload: &[u8], removed: &[usize]) -> Result<Vec<u8>> {
    if payload.len() < 8 {
        return Err(failed("short ipma"));
    }
    let version = payload[0];
    let flags = u32::from_be_bytes([0, payload[1], payload[2], payload[3]]);
    let wide = flags & 1 != 0;
    let id_size = if version < 1 { 2 } else { 4 };
    let prop_size = if wide { 2 } else { 1 };
    let entry_count = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    let mut i = 8;
    let mut out = payload[..8].to_vec();
    for _ in 0..entry_count {
        if i + id_size + 1 > payload.len() {
            return Err(failed("truncated ipma"));
        }
        out.extend_from_slice(&payload[i..i + id_size]);
        i += id_size;
        let count = payload[i] as usize;
        i += 1;
        let count_at = out.len();
        out.push(0);
        let mut kept = 0u8;
        for _ in 0..count {
            if i + prop_size > payload.len() {
                return Err(failed("truncated ipma associations"));
            }
            let (essential, index) = if wide {
                let v = u16::from_be_bytes(payload[i..i + 2].try_into().unwrap());
                (v & 0x8000 != 0, (v & 0x7fff) as usize)
            } else {
                (payload[i] & 0x80 != 0, (payload[i] & 0x7f) as usize)
            };
            i += prop_size;
            if removed.contains(&index) {
                continue;
            }
            let shifted = index - removed.iter().filter(|r| **r < index).count();
            if wide {
                let v = (shifted as u16) | (u16::from(essential) << 15);
                out.extend_from_slice(&v.to_be_bytes());
            } else {
                out.push((u8::from(essential) << 7) | shifted as u8);
            }
            kept += 1;
        }
        out[count_at] = kept;
    }
    Ok(out)
}

/// HEIF Exif item: 4-byte TIFF offset (0) then a TIFF with Orientation.
#[cfg(test)]
fn exif_item_payload(orientation: u8) -> Vec<u8> {
    let mut payload = 0u32.to_be_bytes().to_vec();
    payload.extend_from_slice(&tiff_orientation(orientation));
    payload
}

/// Byte range of the first `Exif` item in `data`, if any.
#[cfg(test)]
fn heif_exif_item_extent(data: &[u8]) -> Option<(usize, usize)> {
    let mut found = None;
    for_each_box(data, |typ, payload| {
        if found.is_some() {
            return;
        }
        if typ == *b"meta" && payload.len() > 4 {
            found = heif_exif_extent_in_meta(&payload[4..], data);
        }
    });
    found
}

#[cfg(test)]
fn heif_exif_extent_in_meta(meta: &[u8], file: &[u8]) -> Option<(usize, usize)> {
    let mut exif_id = None;
    let mut locations: Vec<(u32, u64, u64)> = Vec::new();
    for_each_box(meta, |typ, payload| {
        if typ == *b"iinf" && payload.len() > 4 {
            exif_id = exif_id.or_else(|| exif_item_id(&payload[4..]));
        }
        if typ == *b"iloc" {
            locations = parse_iloc(payload);
        }
    });
    let id = exif_id?;
    let (_, offset, length) = locations.into_iter().find(|(item, _, _)| *item == id)?;
    let start = offset as usize;
    let end = start.saturating_add(length as usize);
    if end > file.len() || start >= file.len() {
        return None;
    }
    Some((start, end - start))
}

/// Writes an Exif Orientation tag as a HEIF `Exif` item, without adding
/// `irot`/`imir`. Replaces an existing Exif item in place when one is present.
#[cfg(test)]
pub(crate) fn inject_heif_exif(data: &[u8], orientation: u8) -> Result<Vec<u8>> {
    let payload = exif_item_payload(orientation);
    if let Some((offset, length)) = heif_exif_item_extent(data) {
        if length < payload.len() {
            return Err(failed("existing Exif item is too small to replace"));
        }
        let mut out = data.to_vec();
        out[offset..offset + payload.len()].copy_from_slice(&payload);
        return Ok(out);
    }
    append_heif_exif_item(data, &payload)
}

#[cfg(test)]
fn append_heif_exif_item(data: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
    let item_id = next_heif_item_id(data)?;
    let primary = pitm_id(data).unwrap_or(1);
    let mut out = data.to_vec();

    let iinf = find_located(&out, b"iinf").ok_or_else(|| failed("HEIF file has no iinf box"))?;
    let infe = make_infe(item_id, first_infe_version(&out[iinf.payload..iinf.end]))?;
    let insert_at = iinf.end;
    out.splice(insert_at..insert_at, infe.iter().copied());
    bump_iinf_count(&mut out, iinf.payload)?;
    add_to_size(&mut out, iinf.offset, infe.len() as i64)?;
    let meta = find_located(&out, b"meta").ok_or_else(|| failed("HEIF file has no meta box"))?;
    add_to_size(&mut out, meta.offset, infe.len() as i64)?;
    bump_iloc_offsets(&mut out, insert_at, infe.len() as i64)?;

    add_iref_cdsc(&mut out, item_id, primary)?;

    let iloc = find_located(&out, b"iloc").ok_or_else(|| failed("HEIF file has no iloc box"))?;
    let entry = make_iloc_entry(
        &out[iloc.payload..iloc.end],
        item_id,
        0,
        payload.len() as u64,
    )?;
    let insert_at = iloc.end;
    out.splice(insert_at..insert_at, entry.iter().copied());
    bump_iloc_item_count(&mut out, iloc.payload)?;
    add_to_size(&mut out, iloc.offset, entry.len() as i64)?;
    let meta = find_located(&out, b"meta").ok_or_else(|| failed("HEIF file has no meta box"))?;
    add_to_size(&mut out, meta.offset, entry.len() as i64)?;
    bump_iloc_offsets(&mut out, insert_at, entry.len() as i64)?;

    let extent = out.len() + 8;
    out.extend_from_slice(&heif_box(b"mdat", payload));
    patch_iloc_item_offset(&mut out, item_id, extent as u64)?;
    Ok(out)
}

#[cfg(test)]
fn pitm_id(data: &[u8]) -> Option<u32> {
    let loc = find_located(data, b"pitm")?;
    let payload = &data[loc.payload..loc.end];
    if payload.len() < 6 {
        return None;
    }
    if payload[0] == 0 {
        Some(u16::from_be_bytes(payload[4..6].try_into().ok()?) as u32)
    } else if payload.len() >= 8 {
        Some(u32::from_be_bytes(payload[4..8].try_into().ok()?))
    } else {
        None
    }
}

#[cfg(test)]
fn iinf_entries(payload: &[u8]) -> &[u8] {
    if payload.len() < 6 {
        return &[];
    }
    if payload[0] == 0 {
        &payload[6..]
    } else if payload.len() >= 8 {
        &payload[8..]
    } else {
        &[]
    }
}

#[cfg(test)]
fn next_heif_item_id(data: &[u8]) -> Result<u32> {
    let mut max = pitm_id(data).unwrap_or(0);
    if let Some(iinf) = find_located(data, b"iinf") {
        let mut rest = iinf_entries(&data[iinf.payload..iinf.end]);
        while rest.len() >= 8 {
            let size = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            if size < 8 || size > rest.len() {
                break;
            }
            if &rest[4..8] == b"infe" {
                let body = &rest[8..size];
                if !body.is_empty() {
                    let version = body[0];
                    let id = if version >= 3 && body.len() >= 8 {
                        Some(u32::from_be_bytes(body[4..8].try_into().unwrap()))
                    } else if version >= 2 && body.len() >= 6 {
                        Some(u16::from_be_bytes(body[4..6].try_into().unwrap()) as u32)
                    } else {
                        None
                    };
                    if let Some(id) = id {
                        max = max.max(id);
                    }
                }
            }
            rest = &rest[size..];
        }
    }
    if max == 0 {
        return Err(failed("HEIF file has no items"));
    }
    max.checked_add(1)
        .ok_or_else(|| failed("HEIF item id overflow"))
}

#[cfg(test)]
fn first_infe_version(iinf_payload: &[u8]) -> u8 {
    let entries = iinf_entries(iinf_payload);
    if entries.len() >= 9 && &entries[4..8] == b"infe" {
        entries[8]
    } else {
        2
    }
}

#[cfg(test)]
fn make_infe(item_id: u32, version: u8) -> Result<Vec<u8>> {
    let version = if item_id > u32::from(u16::MAX) {
        3
    } else {
        version.max(2)
    };
    let mut body = vec![version, 0, 0, 0];
    if version >= 3 {
        body.extend_from_slice(&item_id.to_be_bytes());
    } else {
        body.extend_from_slice(&(item_id as u16).to_be_bytes());
    }
    body.extend_from_slice(&[0, 0]);
    body.extend_from_slice(b"Exif");
    body.push(0);
    Ok(heif_box(b"infe", &body))
}

#[cfg(test)]
fn bump_iinf_count(file: &mut [u8], payload: usize) -> Result<()> {
    if payload + 6 > file.len() {
        return Err(failed("short iinf"));
    }
    if file[payload] == 0 {
        let n = u16::from_be_bytes(file[payload + 4..payload + 6].try_into().unwrap());
        file[payload + 4..payload + 6].copy_from_slice(
            &n.checked_add(1)
                .ok_or_else(|| failed("iinf entry overflow"))?
                .to_be_bytes(),
        );
    } else {
        if payload + 8 > file.len() {
            return Err(failed("short iinf"));
        }
        let n = u32::from_be_bytes(file[payload + 4..payload + 8].try_into().unwrap());
        file[payload + 4..payload + 8].copy_from_slice(
            &n.checked_add(1)
                .ok_or_else(|| failed("iinf entry overflow"))?
                .to_be_bytes(),
        );
    }
    Ok(())
}

#[cfg(test)]
fn make_iloc_entry(iloc_payload: &[u8], item_id: u32, offset: u64, length: u64) -> Result<Vec<u8>> {
    if iloc_payload.len() < 8 {
        return Err(failed("short iloc"));
    }
    let version = iloc_payload[0];
    let sizes = iloc_payload[4];
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0xf) as usize;
    let base_offset_size = (iloc_payload[5] >> 4) as usize;
    if length_size == 0 {
        return Err(failed("iloc has no length field"));
    }
    if offset_size == 0 && base_offset_size == 0 {
        return Err(failed("iloc has no offset field"));
    }
    let mut entry = Vec::new();
    if version < 2 {
        if item_id > u32::from(u16::MAX) {
            return Err(failed("item id does not fit iloc version"));
        }
        entry.extend_from_slice(&(item_id as u16).to_be_bytes());
    } else {
        entry.extend_from_slice(&item_id.to_be_bytes());
    }
    if version == 1 || version == 2 {
        entry.extend_from_slice(&[0, 0]);
    }
    entry.extend_from_slice(&[0, 0]);
    if offset_size == 0 {
        write_iloc_field(&mut entry, base_offset_size, offset)?;
        entry.extend_from_slice(&1u16.to_be_bytes());
        write_iloc_field(&mut entry, length_size, length)?;
    } else {
        write_iloc_field(&mut entry, base_offset_size, 0)?;
        entry.extend_from_slice(&1u16.to_be_bytes());
        write_iloc_field(&mut entry, offset_size, offset)?;
        write_iloc_field(&mut entry, length_size, length)?;
    }
    Ok(entry)
}

#[cfg(test)]
fn write_iloc_field(out: &mut Vec<u8>, size: usize, value: u64) -> Result<()> {
    match size {
        0 => Ok(()),
        4 if value <= u64::from(u32::MAX) => {
            out.extend_from_slice(&(value as u32).to_be_bytes());
            Ok(())
        }
        8 => {
            out.extend_from_slice(&value.to_be_bytes());
            Ok(())
        }
        _ => Err(failed("unsupported iloc field size")),
    }
}

#[cfg(test)]
fn bump_iloc_item_count(file: &mut [u8], payload: usize) -> Result<()> {
    if payload + 8 > file.len() {
        return Err(failed("short iloc"));
    }
    if file[payload] < 2 {
        let n = u16::from_be_bytes(file[payload + 6..payload + 8].try_into().unwrap());
        file[payload + 6..payload + 8].copy_from_slice(
            &n.checked_add(1)
                .ok_or_else(|| failed("iloc item overflow"))?
                .to_be_bytes(),
        );
    } else {
        if payload + 10 > file.len() {
            return Err(failed("short iloc"));
        }
        let n = u32::from_be_bytes(file[payload + 6..payload + 10].try_into().unwrap());
        file[payload + 6..payload + 10].copy_from_slice(
            &n.checked_add(1)
                .ok_or_else(|| failed("iloc item overflow"))?
                .to_be_bytes(),
        );
    }
    Ok(())
}

#[cfg(test)]
fn patch_iloc_item_offset(file: &mut [u8], item_id: u32, offset: u64) -> Result<()> {
    let loc = find_located(file, b"iloc").ok_or_else(|| failed("HEIF file has no iloc box"))?;
    let payload = loc.payload;
    if payload + 8 > file.len() {
        return Err(failed("short iloc"));
    }
    let version = file[payload];
    let sizes = file[payload + 4];
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0xf) as usize;
    let base_offset_size = (file[payload + 5] >> 4) as usize;
    let index_size = if version == 1 || version == 2 {
        (file[payload + 5] & 0xf) as usize
    } else {
        0
    };
    let (mut i, item_count) = if version < 2 {
        (
            payload + 8,
            u16::from_be_bytes(file[payload + 6..payload + 8].try_into().unwrap()) as u32,
        )
    } else {
        (
            payload + 10,
            u32::from_be_bytes(file[payload + 6..payload + 10].try_into().unwrap()),
        )
    };
    let id_size = if version < 2 { 2 } else { 4 };
    for _ in 0..item_count {
        let id = if id_size == 2 {
            u16::from_be_bytes(file[i..i + 2].try_into().unwrap()) as u32
        } else {
            u32::from_be_bytes(file[i..i + 4].try_into().unwrap())
        };
        i += id_size;
        if version == 1 || version == 2 {
            i += 2;
        }
        i += 2;
        let base_at = i;
        i += base_offset_size;
        if i + 2 > file.len() {
            break;
        }
        let extents = u16::from_be_bytes(file[i..i + 2].try_into().unwrap());
        i += 2;
        for _ in 0..extents {
            i += index_size;
            if id == item_id {
                if offset_size == 0 {
                    write_iloc_field_at(&mut file[base_at..], base_offset_size, offset)?;
                } else {
                    write_iloc_field_at(&mut file[i..], offset_size, offset)?;
                }
                return Ok(());
            }
            i += offset_size + length_size;
        }
    }
    Err(failed("iloc is missing the new Exif item"))
}

#[cfg(test)]
fn write_iloc_field_at(slot: &mut [u8], size: usize, value: u64) -> Result<()> {
    match size {
        4 if value <= u64::from(u32::MAX) && slot.len() >= 4 => {
            slot[..4].copy_from_slice(&(value as u32).to_be_bytes());
            Ok(())
        }
        8 if slot.len() >= 8 => {
            slot[..8].copy_from_slice(&value.to_be_bytes());
            Ok(())
        }
        _ => Err(failed("cannot patch iloc offset")),
    }
}

#[cfg(test)]
fn add_iref_cdsc(out: &mut Vec<u8>, from_id: u32, to_id: u32) -> Result<()> {
    if let Some(iref) = find_located(out, b"iref") {
        let version = out[iref.payload];
        let cdsc = make_cdsc(version, from_id, to_id)?;
        let insert_at = iref.end;
        out.splice(insert_at..insert_at, cdsc.iter().copied());
        add_to_size(out, iref.offset, cdsc.len() as i64)?;
        let meta = find_located(out, b"meta").ok_or_else(|| failed("HEIF file has no meta box"))?;
        add_to_size(out, meta.offset, cdsc.len() as i64)?;
        bump_iloc_offsets(out, insert_at, cdsc.len() as i64)?;
    } else {
        let version = u8::from(from_id > u32::from(u16::MAX) || to_id > u32::from(u16::MAX));
        let cdsc = make_cdsc(version, from_id, to_id)?;
        let mut payload = vec![version, 0, 0, 0];
        payload.extend_from_slice(&cdsc);
        let iref = heif_box(b"iref", &payload);
        let meta = find_located(out, b"meta").ok_or_else(|| failed("HEIF file has no meta box"))?;
        let insert_at = meta.end;
        out.splice(insert_at..insert_at, iref.iter().copied());
        add_to_size(out, meta.offset, iref.len() as i64)?;
        bump_iloc_offsets(out, insert_at, iref.len() as i64)?;
    }
    Ok(())
}

#[cfg(test)]
fn make_cdsc(version: u8, from_id: u32, to_id: u32) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    if version == 0 {
        payload.extend_from_slice(&(from_id as u16).to_be_bytes());
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(&(to_id as u16).to_be_bytes());
    } else {
        payload.extend_from_slice(&from_id.to_be_bytes());
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(&to_id.to_be_bytes());
    }
    Ok(heif_box(b"cdsc", &payload))
}

#[cfg(test)]
fn heif_box(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(typ);
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
struct Located {
    offset: usize,
    payload: usize,
    end: usize,
}

#[cfg(test)]
fn find_located(data: &[u8], name: &[u8; 4]) -> Option<Located> {
    let mut found = None;
    walk_located(data, 0, true, &mut |loc, typ| {
        if found.is_none() && typ == *name {
            found = Some(loc);
        }
    });
    found
}

#[cfg(test)]
fn walk_located(data: &[u8], base: usize, recurse: bool, visit: &mut dyn FnMut(Located, [u8; 4])) {
    let mut i = 0;
    while i + 8 <= data.len() {
        let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let typ: [u8; 4] = data[i + 4..i + 8].try_into().unwrap();
        let (header, end) = if size == 1 {
            if i + 16 > data.len() {
                break;
            }
            let large = u64::from_be_bytes(data[i + 8..i + 16].try_into().unwrap()) as usize;
            (16, i.saturating_add(large))
        } else if size == 0 {
            (8, data.len())
        } else {
            (8, i.saturating_add(size))
        };
        if end > data.len() || end < i + header {
            break;
        }
        visit(
            Located {
                offset: base + i,
                payload: base + i + header,
                end: base + end,
            },
            typ,
        );
        if recurse && matches!(&typ, b"meta" | b"iprp") {
            let extra = if typ == *b"meta" { 4 } else { 0 };
            if i + header + extra < end {
                walk_located(
                    &data[i + header + extra..end],
                    base + i + header + extra,
                    true,
                    visit,
                );
            }
        }
        if size == 0 {
            break;
        }
        i = end;
    }
}

#[cfg(test)]
fn count_child_boxes(payload: &[u8]) -> usize {
    let mut n = 0;
    walk_located(payload, 0, false, &mut |_, _| n += 1);
    n
}

#[cfg(test)]
fn add_ipma_properties(payload: &[u8], first_new: usize, added: usize) -> Result<Vec<u8>> {
    if payload.len() < 8 {
        return Err(failed("short ipma"));
    }
    let version = payload[0];
    let flags = u32::from_be_bytes([0, payload[1], payload[2], payload[3]]);
    let wide = flags & 1 != 0;
    let id_size = if version < 1 { 2 } else { 4 };
    let prop_size = if wide { 2 } else { 1 };
    let entry_count = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    let mut i = 8;
    let mut out = payload[..8].to_vec();
    for entry in 0..entry_count {
        if i + id_size + 1 > payload.len() {
            return Err(failed("truncated ipma"));
        }
        out.extend_from_slice(&payload[i..i + id_size + 1]);
        i += id_size;
        let count = payload[i] as usize;
        i += 1;
        let assoc = count * prop_size;
        if i + assoc > payload.len() {
            return Err(failed("truncated ipma associations"));
        }
        out.extend_from_slice(&payload[i..i + assoc]);
        i += assoc;
        if entry == 0 {
            let count_at = out.len() - assoc - 1;
            out[count_at] = (count + added) as u8;
            for n in 0..added {
                let index = first_new + 1 + n; // 1-based
                if wide {
                    let v = (index as u16) | 0x8000;
                    out.extend_from_slice(&v.to_be_bytes());
                } else {
                    out.push(0x80 | (index as u8));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
fn add_to_size(file: &mut [u8], offset: usize, delta: i64) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let size = u32::from_be_bytes(file[offset..offset + 4].try_into().unwrap());
    if size == 0 || size == 1 {
        return Err(failed("extended box sizes are unsupported"));
    }
    let new = i64::from(size) + delta;
    if new < 8 || new > i64::from(u32::MAX) {
        return Err(failed("box size overflow"));
    }
    file[offset..offset + 4].copy_from_slice(&(new as u32).to_be_bytes());
    Ok(())
}

#[cfg(test)]
fn bump_iloc_offsets(file: &mut [u8], insert_at: usize, delta: i64) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let Some(loc) = find_located(file, b"iloc") else {
        return Ok(());
    };
    let payload = loc.payload;
    if payload + 8 > file.len() {
        return Ok(());
    }
    let version = file[payload];
    let sizes = file[payload + 4];
    let offset_size = (sizes >> 4) as usize;
    let length_size = (sizes & 0xf) as usize;
    let base_offset_size = (file[payload + 5] >> 4) as usize;
    let index_size = if version == 1 || version == 2 {
        (file[payload + 5] & 0xf) as usize
    } else {
        0
    };
    let (mut i, item_count) = if version < 2 {
        (
            payload + 8,
            u16::from_be_bytes(file[payload + 6..payload + 8].try_into().unwrap()) as u32,
        )
    } else {
        (
            payload + 10,
            u32::from_be_bytes(file[payload + 6..payload + 10].try_into().unwrap()),
        )
    };
    let id_size = if version < 2 { 2 } else { 4 };
    for _ in 0..item_count {
        i += id_size;
        if version == 1 || version == 2 {
            i += 2;
        }
        i += 2;
        if base_offset_size == 4 && i + 4 <= file.len() {
            bump_u32(&mut file[i..i + 4], insert_at as u64, delta);
        } else if base_offset_size == 8 && i + 8 <= file.len() {
            bump_u64(&mut file[i..i + 8], insert_at as u64, delta);
        }
        i += base_offset_size;
        if i + 2 > file.len() {
            break;
        }
        let extents = u16::from_be_bytes(file[i..i + 2].try_into().unwrap());
        i += 2;
        for _ in 0..extents {
            i += index_size;
            if offset_size == 4 && i + 4 <= file.len() {
                bump_u32(&mut file[i..i + 4], insert_at as u64, delta);
            } else if offset_size == 8 && i + 8 <= file.len() {
                bump_u64(&mut file[i..i + 8], insert_at as u64, delta);
            }
            i += offset_size + length_size;
        }
    }
    Ok(())
}

#[cfg(test)]
fn bump_u32(slot: &mut [u8], insert_at: u64, delta: i64) {
    let v = u32::from_be_bytes(slot[..4].try_into().unwrap()) as u64;
    if v >= insert_at {
        let n = v as i64 + delta;
        if n >= 0 {
            slot[..4].copy_from_slice(&(n as u32).to_be_bytes());
        }
    }
}

#[cfg(test)]
fn bump_u64(slot: &mut [u8], insert_at: u64, delta: i64) {
    let v = u64::from_be_bytes(slot[..8].try_into().unwrap());
    if v >= insert_at {
        let n = v as i64 + delta;
        if n >= 0 {
            slot[..8].copy_from_slice(&(n as u64).to_be_bytes());
        }
    }
}

/// Inserts an APP1 Exif orientation tag after the JPEG SOI.
#[cfg(test)]
pub(crate) fn jpeg_with_orientation(jpeg: &[u8], orientation: u8) -> Result<Vec<u8>> {
    if jpeg.len() < 2 || jpeg[..2] != [0xff, 0xd8] {
        return Err(failed("not a JPEG"));
    }
    let mut out = vec![0xff, 0xd8];
    out.extend_from_slice(&jpeg_exif_app1(orientation));
    out.extend_from_slice(&jpeg[2..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heif_irot_imir_match_exif() {
        let cases = [
            (0, None, Orientation::NoTransforms),
            (3, None, Orientation::Rotate90),
            (2, None, Orientation::Rotate180),
            (1, None, Orientation::Rotate270),
            (0, Some(1), Orientation::FlipHorizontal),
            (0, Some(0), Orientation::FlipVertical),
            (3, Some(1), Orientation::Rotate90FlipH),
            (1, Some(1), Orientation::Rotate270FlipH),
        ];
        for (angle, imir, want) in cases {
            assert_eq!(
                heif_transforms_to_orientation(angle, imir),
                want,
                "irot={angle} imir={imir:?}"
            );
            assert_eq!(
                want.to_exif(),
                match (angle, imir) {
                    (0, None) => 1,
                    (0, Some(1)) => 2,
                    (2, None) => 3,
                    (0, Some(0)) => 4,
                    (3, Some(1)) => 5,
                    (3, None) => 6,
                    (1, Some(1)) => 7,
                    (1, None) => 8,
                    _ => want.to_exif(),
                }
            );
        }
    }

    #[test]
    fn tiff_orientation_round_trips() {
        for tag in 1..=8 {
            let ori = Orientation::from_exif(tag).unwrap();
            assert_eq!(
                orientation_from_exif_bytes(&tiff_orientation(tag)),
                Some(ori)
            );
            let mut prefixed = (6u32).to_be_bytes().to_vec();
            prefixed.extend_from_slice(b"Exif\0\0");
            prefixed.extend_from_slice(&tiff_orientation(tag));
            assert_eq!(orientation_from_exif_bytes(&prefixed), Some(ori));
        }
    }

    fn box_of(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(typ);
        out.extend_from_slice(payload);
        out
    }

    fn full(typ: &[u8; 4], version: u8, rest: &[u8]) -> Vec<u8> {
        let mut payload = vec![version, 0, 0, 0];
        payload.extend_from_slice(rest);
        box_of(typ, &payload)
    }

    /// A decode-invalid but structurally valid still HEIF for box-level tests.
    fn minimal_still_heif() -> Vec<u8> {
        let media = vec![0u8; 16];
        let mut ftyp_payload = Vec::from(*b"heic");
        ftyp_payload.extend_from_slice(&0u32.to_be_bytes());
        ftyp_payload.extend_from_slice(b"mif1heic");
        let ftyp = box_of(b"ftyp", &ftyp_payload);

        let mut hdlr_rest = vec![0u8; 4];
        hdlr_rest.extend_from_slice(b"pict");
        hdlr_rest.extend_from_slice(&[0u8; 12]);
        hdlr_rest.push(0);
        let hdlr = full(b"hdlr", 0, &hdlr_rest);
        let pitm = full(b"pitm", 0, &1u16.to_be_bytes());

        let mut infe_body = vec![2, 0, 0, 0];
        infe_body.extend_from_slice(&1u16.to_be_bytes());
        infe_body.extend_from_slice(&0u16.to_be_bytes());
        infe_body.extend_from_slice(b"hvc1");
        infe_body.push(0);
        let infe = box_of(b"infe", &infe_body);
        let mut iinf_rest = 1u16.to_be_bytes().to_vec();
        iinf_rest.extend_from_slice(&infe);
        let iinf = full(b"iinf", 0, &iinf_rest);

        let mut ispe_rest = Vec::new();
        ispe_rest.extend_from_slice(&64u32.to_be_bytes());
        ispe_rest.extend_from_slice(&48u32.to_be_bytes());
        let ispe = full(b"ispe", 0, &ispe_rest);
        let ipco = box_of(b"ipco", &ispe);
        let mut ipma_rest = 1u32.to_be_bytes().to_vec();
        ipma_rest.extend_from_slice(&1u16.to_be_bytes());
        ipma_rest.push(1);
        ipma_rest.push(0x81);
        let ipma = full(b"ipma", 0, &ipma_rest);
        let iprp = box_of(b"iprp", &[ipco, ipma].concat());

        let iloc_box_size = 8 + 4 + 2 + 2 + 14;
        let children_len = hdlr.len() + pitm.len() + iloc_box_size + iinf.len() + iprp.len();
        let meta_len = 8 + 4 + children_len;
        let mdat_offset = ftyp.len() + meta_len + 8;

        let mut iloc_rest = vec![0x44, 0x00];
        iloc_rest.extend_from_slice(&1u16.to_be_bytes());
        iloc_rest.extend_from_slice(&1u16.to_be_bytes());
        iloc_rest.extend_from_slice(&0u16.to_be_bytes());
        iloc_rest.extend_from_slice(&1u16.to_be_bytes());
        iloc_rest.extend_from_slice(&(mdat_offset as u32).to_be_bytes());
        iloc_rest.extend_from_slice(&(media.len() as u32).to_be_bytes());
        let iloc = full(b"iloc", 0, &iloc_rest);
        assert_eq!(iloc.len(), iloc_box_size);

        let mut meta_payload = vec![0, 0, 0, 0];
        meta_payload.extend_from_slice(&hdlr);
        meta_payload.extend_from_slice(&pitm);
        meta_payload.extend_from_slice(&iloc);
        meta_payload.extend_from_slice(&iinf);
        meta_payload.extend_from_slice(&iprp);
        let meta = box_of(b"meta", &meta_payload);
        let mdat = box_of(b"mdat", &media);

        let mut file = ftyp;
        file.extend_from_slice(&meta);
        file.extend_from_slice(&mdat);
        file
    }

    #[test]
    fn ipco_irot_is_found() {
        let irot = box_of(b"irot", &[3]);
        let imir = box_of(b"imir", &[1]);
        let ipco = box_of(b"ipco", &[irot, imir].concat());
        let iprp = box_of(b"iprp", &ipco);
        let mut meta_payload = vec![0, 0, 0, 0];
        meta_payload.extend_from_slice(&iprp);
        let meta = box_of(b"meta", &meta_payload);
        assert_eq!(
            heif_display_orientation(&meta),
            Some(Orientation::Rotate90FlipH)
        );
    }

    #[test]
    fn inject_heif_exif_round_trips() {
        let base = minimal_still_heif();
        assert!(heif_exif_orientation(&base).is_none());
        let with_exif = inject_heif_exif(&base, 6).unwrap();
        assert_eq!(
            heif_exif_orientation(&with_exif),
            Some(Orientation::Rotate90)
        );
        assert!(heif_display_orientation(&with_exif).is_none());

        let replaced = inject_heif_exif(&with_exif, 3).unwrap();
        assert_eq!(
            heif_exif_orientation(&replaced),
            Some(Orientation::Rotate180)
        );

        let with_irot = inject_heif_transforms(&base, Some(3), None).unwrap();
        assert_eq!(
            heif_display_orientation(&with_irot),
            Some(Orientation::Rotate90)
        );
        let iphone = inject_heif_exif(&with_irot, 6).unwrap();
        assert_eq!(
            heif_display_orientation(&iphone),
            Some(Orientation::Rotate90),
            "adding EXIF must keep irot"
        );
        assert_eq!(
            heif_exif_orientation(&iphone).map(Orientation::to_exif),
            Some(6)
        );

        let with_identity = inject_heif_transforms(&base, Some(0), None).unwrap();
        assert!(
            heif_display_orientation(&with_identity).is_none(),
            "identity irot must not block EXIF"
        );
        let stripped = strip_heif_transforms(&with_irot).unwrap();
        assert!(
            heif_display_orientation(&stripped).is_none(),
            "strip must drop irot"
        );
        assert_eq!(
            heif_exif_orientation(&inject_heif_exif(&stripped, 6).unwrap())
                .map(Orientation::to_exif),
            Some(6)
        );
    }
}
