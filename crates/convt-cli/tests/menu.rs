//! `convt menu` and `convt targets --menu` as the binary runs them. Every
//! test gets its own config folder, so the user's `menu.toml` is never read.
//! Only image formats are used: the `image` engine is always there.

use std::path::PathBuf;
use std::process::Output;

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.path().join("menu.toml")
    }

    fn saved(&self) -> String {
        std::fs::read_to_string(self.file()).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_convt"))
            .args(args)
            .env("CONVT_CONFIG_DIR", self.dir.path())
            .env("CONVT_DATA_DIR", self.dir.path().join("data"))
            .env("CONVT_LICENSE_STORE", "file")
            .env("CONVT_LICENSE_ENFORCE", "0")
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn menu_for(&self, name: &str) -> String {
        self.ok(&["targets", name, "--menu"])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn line<'a>(show: &'a str, kind: &str) -> &'a str {
    show.lines()
        .find(|l| l.split_whitespace().next() == Some(kind))
        .unwrap_or_else(|| panic!("no {kind} line in {show}"))
}

#[test]
fn set_keeps_the_order_and_menus_follow_it() {
    let f = Fixture::new();
    let defaults = f.menu_for("x.bmp");
    assert!(line(&f.ok(&["menu"]), "image").ends_with("(default)"));

    f.ok(&["menu", "set", "image", "webp", "tiff", "webp"]);
    assert_eq!(f.saved().trim(), r#"image = ["webp", "tiff"]"#);
    let show = f.ok(&["menu", "show"]);
    assert_eq!(
        line(&show, "image").split_whitespace().collect::<Vec<_>>(),
        ["image", "webp", "tiff"]
    );
    assert!(line(&show, "vector").ends_with("(default)"));
    assert_eq!(f.menu_for("x.bmp"), "webp tiff");
    // A file never offers its own format.
    assert_eq!(f.menu_for("x.webp"), "tiff");
    assert_ne!(f.menu_for("x.bmp"), defaults);
}

#[test]
fn rejected_changes_leave_the_file_alone() {
    let f = Fixture::new();
    f.ok(&["menu", "set", "image", "png"]);
    let before = f.saved();
    for args in [
        &["menu", "set", "nope", "png"][..],
        &["menu", "set", "image", "zzz"],
        &["menu", "set", "image", "mp3"],
        &["menu", "reset", "nope"],
    ] {
        let out = f.run(args);
        assert!(!out.status.success(), "{args:?} should fail");
        assert!(!stderr(&out).is_empty(), "{args:?} says why");
        assert_eq!(f.saved(), before, "{args:?}");
    }
    assert!(stderr(&f.run(&["menu", "set", "nope", "png"])).contains("image"));
}

#[test]
fn reset_one_kind_keeps_the_others_and_reset_all_repairs_the_file() {
    let f = Fixture::new();
    f.ok(&["menu", "set", "image", "tiff"]);
    f.ok(&["menu", "set", "vector", "webp"]);
    f.ok(&["menu", "reset", "image"]);
    assert_eq!(f.saved().trim(), r#"vector = ["webp"]"#);

    std::fs::write(f.file(), "image = 3").unwrap();
    let out = f.run(&["menu", "show"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("menu.toml"), "{}", stderr(&out));
    // Menus fall back to the defaults instead of failing.
    assert!(!f.menu_for("x.bmp").is_empty());

    f.ok(&["menu", "reset"]);
    assert!(line(&f.ok(&["menu"]), "image").ends_with("(default)"));
}
