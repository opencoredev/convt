//! PDF output from images and text output from PDFs, end to end through the
//! registry. Every case needs PDFium and prints SKIP without it.

use std::path::{Path, PathBuf};

use convt_core::{
    Background, Cancel, Error, Job, Options, Output, PageRange, Registry, format_by_id,
};

fn registry() -> Option<Registry> {
    let registry = convt_engines::default_registry();
    if registry.engines().iter().any(|e| e.id() == "pdfium") {
        Some(registry)
    } else {
        eprintln!("SKIP: PDFium missing");
        None
    }
}

fn convert(r: &Registry, input: &Path, to: &str, options: Options) -> Result<Vec<PathBuf>, Error> {
    let out = tempfile::Builder::new()
        .prefix("out-")
        .tempdir_in(input.parent().unwrap())
        .unwrap()
        .keep();
    let job = Job {
        output: Output::Dir(out),
        options,
        ..Job::new(input, format_by_id(to).unwrap())
    };
    r.run(&job, &|_| {}, &Cancel::new())
}

/// Renders the only page of `pdf` at 72 DPI, so one pixel is one point.
fn render(r: &Registry, pdf: &Path, background: Option<Background>) -> image::RgbaImage {
    let options = Options {
        dpi: Some(72),
        background,
        ..Options::default()
    };
    let pages = convert(r, pdf, "png", options).unwrap();
    assert_eq!(pages.len(), 1, "one page");
    image::open(&pages[0]).unwrap().to_rgba8()
}

fn near(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 12)
}

/// 300x200: the left half opaque red, the right half transparent.
fn half_transparent(dir: &Path) -> PathBuf {
    let img = image::RgbaImage::from_fn(300, 200, |x, _| {
        if x < 150 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    let path = dir.join("half.png");
    img.save(&path).unwrap();
    path
}

#[test]
fn images_become_one_page_pdfs_at_their_own_size() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    let src = half_transparent(dir.path());
    let pdf = convert(&r, &src, "pdf", Options::default()).unwrap();
    assert_eq!(pdf.len(), 1);
    assert!(std::fs::read(&pdf[0]).unwrap().starts_with(b"%PDF-"));
    // 96 DPI unless chosen: 300x200 pixels are 225x150 points.
    let page = render(&r, &pdf[0], Some(Background::Transparent));
    assert_eq!(page.dimensions(), (225, 150));
    assert!(near(page.get_pixel(50, 75).0, [255, 0, 0, 255]));
    assert_eq!(page.get_pixel(200, 75).0[3], 0, "transparency is kept");
    // Viewers show the white page through it.
    let page = render(&r, &pdf[0], None);
    assert!(near(page.get_pixel(200, 75).0, [255, 255, 255, 255]));

    let at = |options: Options| {
        let pdf = convert(&r, &src, "pdf", options).unwrap();
        render(&r, &pdf[0], Some(Background::Transparent))
    };
    let page = at(Options {
        dpi: Some(144),
        ..Options::default()
    });
    assert_eq!(page.dimensions(), (150, 100));
    let page = at(Options {
        max_size: Some(100),
        dpi: Some(72),
        ..Options::default()
    });
    assert_eq!(page.width(), 100);
    assert!(page.height().abs_diff(67) <= 1, "{}", page.height());
    let page = at(Options {
        background: Some(Background::BLACK),
        ..Options::default()
    });
    assert!(near(page.get_pixel(200, 75).0, [0, 0, 0, 255]), "flattened");
}

/// Converts `img` to a PDF at 72 DPI and renders it back, so each pixel of
/// the page is one pixel of the source.
fn round_trip(r: &Registry, dir: &Path, name: &str, img: image::DynamicImage) -> image::RgbaImage {
    let src = dir.join(name);
    img.save(&src).unwrap();
    let options = Options {
        dpi: Some(72),
        ..Options::default()
    };
    let pdf = convert(r, &src, "pdf", options).unwrap();
    render(r, &pdf[0], None)
}

#[test]
fn opaque_images_keep_every_pixel_in_place() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    // 301 pixels of RGB is 903 bytes a row, so PDFium pads each row. A row
    // read at the wrong stride shears the white column sideways.
    let rgb = image::RgbImage::from_fn(301, 40, |x, _| match x {
        150 => image::Rgb([255, 255, 255]),
        0..150 => image::Rgb([255, 0, 0]),
        _ => image::Rgb([0, 0, 255]),
    });
    let page = round_trip(&r, dir.path(), "rgb.png", rgb.into());
    assert_eq!(page.dimensions(), (301, 40));
    for y in [0, 20, 39] {
        assert!(
            near(page.get_pixel(150, y).0, [255, 255, 255, 255]),
            "y {y}"
        );
        assert!(near(page.get_pixel(148, y).0, [255, 0, 0, 255]), "y {y}");
        assert!(near(page.get_pixel(152, y).0, [0, 0, 255, 255]), "y {y}");
        assert!(near(page.get_pixel(300, y).0, [0, 0, 255, 255]), "y {y}");
    }

    // Gray rows of 37 bytes are padded to 40.
    let gray =
        image::GrayImage::from_fn(37, 30, |x, _| image::Luma([if x == 20 { 255 } else { 40 }]));
    let page = round_trip(&r, dir.path(), "gray.png", gray.into());
    assert_eq!(page.dimensions(), (37, 30));
    for y in [0, 15, 29] {
        assert!(near(page.get_pixel(20, y).0, [255, 255, 255, 255]), "y {y}");
        assert!(near(page.get_pixel(18, y).0, [40, 40, 40, 255]), "y {y}");
        assert!(near(page.get_pixel(22, y).0, [40, 40, 40, 255]), "y {y}");
        assert!(near(page.get_pixel(36, y).0, [40, 40, 40, 255]), "y {y}");
    }
}

/// A 64x32 JPEG, red on the left and blue on the right.
fn jpeg(dir: &Path, name: &str) -> PathBuf {
    let img = image::RgbImage::from_fn(64, 32, |x, _| {
        if x < 32 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let path = dir.join(name);
    img.save(&path).unwrap();
    path
}

#[test]
fn photos_go_in_upright_and_unchanged_when_they_can() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    let plain = jpeg(dir.path(), "plain.jpg");
    let options = Options {
        dpi: Some(72),
        ..Options::default()
    };
    let pdf = convert(&r, &plain, "pdf", options.clone()).unwrap();
    let bytes = std::fs::read(&pdf[0]).unwrap();
    let source = std::fs::read(&plain).unwrap();
    assert!(
        bytes.windows(source.len()).any(|w| w == source),
        "an upright JPEG is embedded byte for byte"
    );
    assert_eq!(render(&r, &pdf[0], None).dimensions(), (64, 32));

    // Exif big-endian TIFF, one SHORT orientation entry: 6 (rotate 90 degrees
    // clockwise to display), so the red left half ends up on top.
    let exif = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";
    let mut oriented = source[..2].to_vec();
    oriented.extend_from_slice(&[0xff, 0xe1]);
    oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    oriented.extend_from_slice(exif);
    oriented.extend_from_slice(&source[2..]);
    let rotated = dir.path().join("rotated.jpg");
    std::fs::write(&rotated, oriented).unwrap();
    let pdf = convert(&r, &rotated, "pdf", options).unwrap();
    let page = render(&r, &pdf[0], None);
    assert_eq!(page.dimensions(), (32, 64));
    assert!(near(page.get_pixel(16, 10).0, [255, 0, 0, 255]));
    assert!(near(page.get_pixel(16, 54).0, [0, 0, 255, 255]));
}

/// A PDF with one page per entry of `pages`, each line at its own height in
/// points from the bottom, set in 12 point Helvetica.
fn text_pdf(dir: &Path, name: &str, pages: &[&[(u32, &str)]]) -> PathBuf {
    let n = pages.len();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n)
                .map(|i| format!("{} 0 R", 4 + 2 * i))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for lines in pages {
        let content: String = lines
            .iter()
            .map(|(y, text)| {
                let text = text
                    .replace('\\', "\\\\")
                    .replace('(', "\\(")
                    .replace(')', "\\)");
                format!("BT /F1 12 Tf 72 {y} Td ({text}) Tj ET\n")
            })
            .collect();
        let contents = 4 + objects.len() - 2;
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {contents} 0 R >>"
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    let mut body = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(body.len());
        body += &format!("{} 0 obj\n{object}\nendobj\n", i + 1);
    }
    let xref = body.len();
    body += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        body += &format!("{offset:010} 00000 n \n");
    }
    body += &format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

fn three_pages(dir: &Path) -> PathBuf {
    text_pdf(
        dir,
        "notes.pdf",
        &[
            // Two lines 14 points apart wrap one paragraph; 40 points down
            // starts another.
            &[
                (700, "First line of a paragraph"),
                (686, "that wraps onto a second."),
                (646, "Fish & chips <b> \"quoted\""),
            ],
            &[(700, "Page two")],
            &[(700, "Page three")],
        ],
    )
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn pdf_text_becomes_one_txt_file_with_form_feeds_between_pages() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    let src = three_pages(dir.path());
    let txt = convert(&r, &src, "txt", Options::default()).unwrap();
    assert_eq!(txt.len(), 1, "one file, not one per page");
    assert_eq!(
        read(&txt[0]),
        "First line of a paragraph\nthat wraps onto a second.\nFish & chips <b> \"quoted\"\n\
         \x0cPage two\n\x0cPage three\n"
    );
    let pages = |range: &str| Options {
        pages: Some(range.parse::<PageRange>().unwrap()),
        ..Options::default()
    };
    let txt = convert(&r, &src, "txt", pages("2-")).unwrap();
    assert_eq!(read(&txt[0]), "Page two\n\x0cPage three\n");
    let err = convert(&r, &src, "txt", pages("7")).unwrap_err();
    assert_eq!(err.kind(), "invalid_option");
    assert!(err.to_string().contains("select nothing"), "{err}");
}

#[test]
fn pdf_text_becomes_a_clean_html_document() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    let src = three_pages(dir.path());
    let html = convert(&r, &src, "html", Options::default()).unwrap();
    assert_eq!(html.len(), 1);
    assert_eq!(
        read(&html[0]),
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>notes</title>\n\
         </head>\n<body>\n\
         <section id=\"page-1\">\n\
         <p>First line of a paragraph that wraps onto a second.</p>\n\
         <p>Fish &amp; chips &lt;b&gt; &quot;quoted&quot;</p>\n</section>\n\
         <section id=\"page-2\">\n<p>Page two</p>\n</section>\n\
         <section id=\"page-3\">\n<p>Page three</p>\n</section>\n</body>\n</html>\n"
    );
    let only_three = Options {
        pages: Some("3".parse().unwrap()),
        ..Options::default()
    };
    let html = read(&convert(&r, &src, "html", only_three).unwrap()[0]);
    assert!(html.contains("<section id=\"page-3\">") && !html.contains("page-1"));
}

#[test]
fn a_pdf_without_text_says_so_instead_of_writing_an_empty_file() {
    let Some(r) = registry() else { return };
    let dir = tempfile::tempdir().unwrap();
    let src = text_pdf(dir.path(), "scan.pdf", &[&[]]);
    for to in ["txt", "html"] {
        let err = convert(&r, &src, to, Options::default()).unwrap_err();
        assert_eq!(err.kind(), "engine_failed");
        assert!(err.to_string().contains("no text"), "{err}");
    }
}

#[test]
fn pdfs_reach_word_processor_formats_through_html() {
    if registry().is_none() {
        return;
    }
    // Registers LibreOffice, or a stand-in declaring its steps without it.
    let r = convt_engines::registry_with_planned_documents();
    let pdf = format_by_id("pdf").unwrap();
    for to in ["docx", "odt", "rtf"] {
        let plan = r.plan(pdf, format_by_id(to).unwrap()).unwrap();
        assert_eq!(plan.hops.len(), 2, "{}", plan.describe());
        assert_eq!(plan.hops[0].0.id(), "pdfium", "{}", plan.describe());
        assert_eq!(plan.hops[0].1.to.id, "html", "{}", plan.describe());
    }
    let menu: Vec<_> = r.menu_targets(pdf).iter().map(|f| f.id).collect();
    assert_eq!(menu, ["png", "jpeg", "docx", "txt"]);
}

#[test]
fn images_offer_pdf_but_never_documents() {
    let Some(r) = registry() else { return };
    let r2 = convt_engines::registry_with_planned_documents();
    for r in [&r, &r2] {
        for from in ["png", "jpeg", "webp", "svg", "heic"] {
            let targets: Vec<_> = r
                .targets(format_by_id(from).unwrap())
                .iter()
                .map(|f| f.id)
                .collect();
            if from != "heic" || !targets.is_empty() {
                assert!(targets.contains(&"pdf"), "{from}: {targets:?}");
            }
            for doc in ["txt", "html", "docx", "odt", "rtf", "doc"] {
                assert!(!targets.contains(&doc), "{from} offers {doc}");
            }
        }
        let menu: Vec<_> = r
            .menu_targets(format_by_id("jpeg").unwrap())
            .iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(menu, ["png", "webp", "pdf"]);
    }
}
