use std::path::{Path, PathBuf};
use std::process::Command;

use convt_core::{Category, Ctx, Engine, Error, Result, Step};

const DOCUMENTS: &[&str] = &["docx", "doc", "odt", "rtf", "txt", "html"];
const PRESENTATIONS: &[&str] = &["pptx", "ppt", "odp"];
const SPREADSHEETS: &[&str] = &["xlsx", "xls", "ods", "csv"];

/// Converts Office documents with headless LibreOffice. LibreOffice is an
/// optional download in the desktop app; the cloud workers always have it.
/// TODO: keep one instance warm to skip the ~1s startup per file.
pub struct OfficeEngine {
    soffice: Option<PathBuf>,
}

impl OfficeEngine {
    pub fn new() -> Self {
        Self {
            soffice: crate::find_tool_with_pack(
                &["soffice", "libreoffice"],
                "CONVT_SOFFICE",
                crate::packs::installed_documents(),
            ),
        }
    }
}

impl Default for OfficeEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn filter(step: Step) -> &'static str {
    // HTML embeds images as data URIs; otherwise they land in companion files
    // that a single-file output can't carry.
    match (step.to.id, step.from.category) {
        // Writer/Web's default extension lookup omits these export filters.
        ("docx", _) => "docx:Office Open XML Text",
        ("doc", _) => "doc:MS Word 97",
        ("rtf", _) => "rtf:Rich Text Format",
        ("txt", _) => "txt:Text (encoded):UTF8",
        ("html", Category::Spreadsheet) => "html:HTML (StarCalc):EmbedImages",
        ("html", _) => "html:HTML (StarWriter):EmbedImages",
        (id, _) => id,
    }
}

impl Engine for OfficeEngine {
    fn id(&self) -> &'static str {
        "libreoffice"
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.soffice.is_none().then(|| {
            if crate::packs::documents_configured() {
                "Document pack not installed; run `convt pack install documents`".into()
            } else {
                "LibreOffice not found; this build has no document pack to download".into()
            }
        })
    }

    fn steps(&self) -> Vec<Step> {
        let with_pdf =
            |set: &[&'static str]| set.iter().copied().chain(["pdf"]).collect::<Vec<_>>();
        crate::steps(DOCUMENTS, &with_pdf(DOCUMENTS))
            .chain(crate::steps(PRESENTATIONS, &with_pdf(PRESENTATIONS)))
            .chain(crate::steps(SPREADSHEETS, &with_pdf(SPREADSHEETS)))
            .collect()
    }

    fn convert(&self, ctx: &Ctx, input: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
        let soffice = self.soffice.as_ref().ok_or(Error::EngineUnavailable {
            engine: "libreoffice",
            reason: "not installed".into(),
        })?;
        ctx.indeterminate();
        // A private profile lets several conversions run at once.
        let profile = tempfile::tempdir()?;
        let outdir = tempfile::tempdir()?;
        let mut cmd = Command::new(soffice);
        cmd.arg(format!(
            "-env:UserInstallation=file://{}",
            profile.path().display()
        ))
        .args([
            "--headless",
            "--norestore",
            "--convert-to",
            filter(ctx.step),
            "--outdir",
        ])
        .arg(outdir.path())
        .arg(input);
        crate::run_tool("libreoffice", cmd, ctx, |_| {})?;
        let produced: Vec<PathBuf> = std::fs::read_dir(outdir.path())?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<_>>()?;
        let is_target = |p: &PathBuf| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                ctx.step
                    .to
                    .extensions
                    .contains(&e.to_ascii_lowercase().as_str())
            })
        };
        // LibreOffice exits 0 even when it could not load the file.
        let Some(main) = produced.iter().position(is_target) else {
            return Err(Error::EngineFailed {
                engine: "libreoffice",
                message: "LibreOffice produced no output; the file may be damaged or protected"
                    .into(),
            });
        };
        if produced.len() > 1 {
            return Err(Error::EngineFailed {
                engine: "libreoffice",
                message: format!(
                    "LibreOffice split the {} into several files, which convt can't save as one",
                    ctx.step.to.name
                ),
            });
        }
        let output = ctx.artifact(out_dir, 0);
        std::fs::copy(&produced[main], &output)?;
        Ok(vec![output])
    }
}
