// SPDX-License-Identifier: GPL-3.0-or-later

//! Qt Quick UI, opened when uvpad is started without an input (or with --gui).
//!
//! The window (qml/main.qml) talks to one `Padder` object. Padding runs on a
//! background thread and writes a temporary preview PNG, which the window
//! shows. Saving copies that file (or re-encodes it with the chosen
//! compression) to wherever the user picks in the file dialog.
//!
//! Inputs that are not PNGs are also shown through a temporary PNG, so the
//! preview never depends on which Qt image format plugins are installed.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QQmlApplicationEngine, QString, QUrl};

use crate::{check_output, default_output, is_png, load, pad, run, write_rgba, Level};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    // src/app.cpp
    unsafe extern "C++" {
        include!("uvpad/src/app.h");
        type QApplication;

        fn uvpad_new_application(version: &str, icon_png: &[u8]) -> UniquePtr<QApplication>;
        fn uvpad_exec() -> i32;
        fn uvpad_set_env(name: &str, value: &str);
        fn uvpad_copy_png(png: &[u8], with_image: bool);
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy)]
        #[qproperty(QString, status)]
        #[qproperty(bool, failed)]
        #[qproperty(QUrl, input_url, cxx_name = "inputUrl")]
        #[qproperty(QUrl, input_preview_url, cxx_name = "inputPreviewUrl")]
        #[qproperty(QUrl, output_url, cxx_name = "outputUrl")]
        #[qproperty(QUrl, suggested_url, cxx_name = "suggestedUrl")]
        #[qproperty(QUrl, input_folder, cxx_name = "inputFolder")]
        #[qproperty(i32, image_width, cxx_name = "imageWidth")]
        #[qproperty(i32, image_height, cxx_name = "imageHeight")]
        #[qproperty(bool, slower)]
        #[qproperty(i32, compression)]
        type Padder = super::PadderRust;

        /// Pads the image at `url` and shows the result.
        #[qinvokable]
        fn process(self: Pin<&mut Padder>, url: &QUrl);

        /// Writes the shown result to `url`.
        #[qinvokable]
        fn save(self: Pin<&mut Padder>, url: &QUrl);

        /// Puts the shown result on the clipboard.
        #[qinvokable]
        #[cxx_name = "copyOutput"]
        fn copy_output(self: Pin<&mut Padder>);

        /// The image given on the command line, if any (only returned once).
        #[qinvokable]
        #[cxx_name = "takeStartupUrl"]
        fn take_startup_url(self: &Padder) -> QUrl;
    }

    impl cxx_qt::Threading for Padder {}
}

/// Image passed as `uvpad --gui <INPUT>`.
static STARTUP: OnceLock<std::sync::Mutex<Option<PathBuf>>> = OnceLock::new();

/// A file in the temp directory, deleted when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(job: u64, what: &str) -> Self {
        let name = format!("uvpad-{what}-{}-{job}.png", std::process::id());
        Self(std::env::temp_dir().join(name))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The padded preview of one input, kept until replaced.
struct Preview {
    input: PathBuf,
    file: TempFile,
    slower: bool,
}

#[derive(Default)]
pub struct PadderRust {
    busy: bool,
    status: QString,
    failed: bool,
    input_url: QUrl,
    /// What the window shows as the input: the input itself if it is a PNG.
    input_preview_url: QUrl,
    output_url: QUrl,
    suggested_url: QUrl,
    input_folder: QUrl,
    /// Size of the input (and output) texture, 0 if unknown.
    image_width: i32,
    image_height: i32,
    slower: bool,
    /// 0 fast, 1 default, 2 best (the order of the combo box)
    compression: i32,

    preview: Option<Preview>,
    /// The PNG copy behind `input_preview_url`, for inputs that are not PNGs.
    input_preview: Option<TempFile>,
    /// Bumped by every job, so results of superseded jobs are dropped.
    job: u64,
}

fn level(compression: i32) -> Level {
    match compression {
        1 => Level::Default,
        2 => Level::Best,
        _ => Level::Fast,
    }
}

fn to_path(url: &QUrl) -> Option<PathBuf> {
    url.to_local_file()
        .map(|s| PathBuf::from(String::from(&s)))
        .filter(|p| !p.as_os_str().is_empty())
}

fn to_url(path: &Path) -> QUrl {
    QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl qobject::Padder {
    fn report(mut self: Pin<&mut Self>, busy: bool, failed: bool, status: String) {
        self.as_mut().set_busy(busy);
        self.as_mut().set_failed(failed);
        self.as_mut().set_status(QString::from(status));
    }

    /// Starts a new job; returns its id.
    fn next_job(mut self: Pin<&mut Self>) -> u64 {
        let mut rust = self.as_mut().rust_mut();
        rust.job += 1;
        rust.job
    }

    pub fn process(mut self: Pin<&mut Self>, url: &QUrl) {
        let Some(input) = to_path(url) else {
            self.report(false, true, "Only local files can be padded".into());
            return;
        };
        let slower = *self.slower();
        let job = self.as_mut().next_job();

        // A different image: clear the old one rather than show it at the new size.
        // (Re-padding the same image keeps it on screen until the new result.)
        let same = self
            .rust()
            .preview
            .as_ref()
            .is_some_and(|p| p.input == input);
        if !same {
            self.as_mut().set_image_width(0);
            self.as_mut().set_image_height(0);
            self.as_mut().set_input_preview_url(QUrl::default());
            self.as_mut().set_output_url(QUrl::default());
        }
        self.as_mut().set_input_url(to_url(&input));
        if let Some(dir) = input.parent() {
            self.as_mut().set_input_folder(to_url(dir));
        }
        self.as_mut()
            .set_suggested_url(to_url(&default_output(&input)));
        self.as_mut()
            .report(true, false, format!("Padding {}…", file_name(&input)));

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let start = Instant::now();
            // The previews only live in the temp directory, so encode them fast.
            let file = TempFile::new(job, "output");
            let mut input_preview = None;
            let result = (|| {
                let img = load(&input)?;
                let shown = if is_png(&input) {
                    input.clone()
                } else {
                    let temp = TempFile::new(job, "input");
                    write_rgba(&temp.0, &img, Level::Fast)
                        .context("failed to write the input preview")?;
                    input_preview.insert(temp).0.clone()
                };
                // Show the input while it is being padded.
                let (width, height) = (img.width as i32, img.height as i32);
                let _ = thread.queue(move |mut padder| {
                    if padder.rust().job == job {
                        padder.as_mut().set_image_width(width);
                        padder.as_mut().set_image_height(height);
                        padder.as_mut().set_input_preview_url(to_url(&shown));
                    }
                });
                pad(img, &file.0, slower, Level::Fast)
            })();
            let elapsed = start.elapsed();
            let preview = Preview {
                input,
                file,
                slower,
            };
            let _ = thread.queue(move |mut padder| {
                if padder.rust().job != job {
                    return; // superseded; dropping the previews deletes their files
                }
                padder.as_mut().rust_mut().input_preview = input_preview;
                match result {
                    Ok(()) => {
                        let status = format!(
                            "Padded {} in {elapsed:.2?}{}",
                            file_name(&preview.input),
                            if slower { " (GIMP UVPad)" } else { "" },
                        );
                        padder.as_mut().set_output_url(to_url(&preview.file.0));
                        padder.as_mut().rust_mut().preview = Some(preview);
                        padder.report(false, false, status);
                    }
                    Err(e) => {
                        padder.as_mut().set_output_url(QUrl::default());
                        padder.as_mut().rust_mut().preview = None;
                        padder.report(false, true, format!("{e:#}"));
                    }
                }
            });
        });
    }

    pub fn save(mut self: Pin<&mut Self>, url: &QUrl) {
        let Some(dest) = to_path(url) else {
            self.report(false, true, "Can only save to a local file".into());
            return;
        };
        if let Err(e) = check_output(&dest) {
            self.report(false, true, format!("{e:#}"));
            return;
        }
        let Some(preview) = &self.rust().preview else {
            self.report(false, true, "Nothing to save yet".into());
            return;
        };
        let (input, file, slower) = (
            preview.input.clone(),
            preview.file.0.clone(),
            preview.slower,
        );
        let level = level(*self.compression());
        let job = self.as_mut().next_job();
        self.as_mut()
            .report(true, false, format!("Saving {}…", file_name(&dest)));

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = match level {
                // The preview already is exactly this file.
                Level::Fast => std::fs::copy(&file, &dest)
                    .map(drop)
                    .with_context(|| format!("failed to write {}", dest.display())),
                // Padding is cheap next to `best` compression, so just redo it.
                _ => run(&input, &dest, slower, level),
            };
            let _ = thread.queue(move |padder| {
                if padder.rust().job != job {
                    return;
                }
                match result {
                    Ok(()) => padder.report(false, false, format!("Saved {}", dest.display())),
                    Err(e) => padder.report(false, true, format!("{e:#}")),
                }
            });
        });
    }

    pub fn copy_output(mut self: Pin<&mut Self>) {
        let Some(preview) = &self.rust().preview else {
            self.report(false, true, "Nothing to copy yet".into());
            return;
        };
        let file = preview.file.0.clone();
        // Decoding for the bitmap copy is slow and memory hungry on big
        // textures, so those only go on the clipboard as a PNG.
        let with_image =
            i64::from(*self.image_width()) * i64::from(*self.image_height()) <= 4096 * 4096;
        self.as_mut().report(true, false, "Copying…".into());

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = std::fs::read(&file);
            let _ = thread.queue(move |padder| match result {
                Ok(png) => {
                    qobject::uvpad_copy_png(&png, with_image);
                    padder.report(
                        false,
                        false,
                        "Copied the padded image to the clipboard".into(),
                    );
                }
                Err(e) => padder.report(false, true, format!("failed to copy: {e}")),
            });
        });
    }

    pub fn take_startup_url(&self) -> QUrl {
        STARTUP
            .get()
            .and_then(|m| m.lock().ok()?.take())
            .map(|p| to_url(&p))
            .unwrap_or_default()
    }
}

/// A double-clicked uvpad.exe gets a console window of its own: close it.
/// Started from a terminal, the console is shared and stays.
#[cfg(windows)]
fn close_own_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        fn FreeConsole() -> i32;
    }
    let mut processes = [0u32; 2];
    // SAFETY: plain Win32 calls; the buffer is as long as the count passed.
    unsafe {
        if GetConsoleProcessList(processes.as_mut_ptr(), 2) == 1 {
            FreeConsole();
        }
    }
}

/// Where the packages put Qt's QML modules, relative to the executable:
/// next to it (Windows), ../qml (AppImage) or ../Resources/qml (macOS).
fn bundled_qml_dirs() -> Vec<PathBuf> {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    ["qml", "../qml", "../Resources/qml"]
        .iter()
        .map(|rel| dir.join(rel))
        .filter(|path| path.join("QtQuick").is_dir())
        .collect()
}

/// Sets a variable for Qt, which on Windows does not see std::env::set_var.
fn set_env(name: &str, value: &str) {
    qobject::uvpad_set_env(name, value);
}

pub fn launch(input: Option<PathBuf>) -> Result<()> {
    #[cfg(windows)]
    close_own_console();

    let input = input
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .filter(|p| !p.as_os_str().is_empty());
    let _ = STARTUP.set(std::sync::Mutex::new(input));

    // Image readers warn about every file with odd metadata (libpng: "eXIf:
    // invalid"), and the file dialog's previews read whole folders of them.
    // Failures still show in the window. Later rules win, so the user's
    // QT_LOGGING_RULES can turn them back on.
    let rules = std::env::var("QT_LOGGING_RULES").unwrap_or_default();
    set_env("QT_LOGGING_RULES", &format!("qt.gui.imageio=false;{rules}"));

    // An AppImage brings its own Qt, which cannot load the desktop's platform
    // theme plugin (KDE's, say); the portal still gives the desktop's dialogs.
    if std::env::var_os("APPIMAGE").is_some() && std::env::var_os("QT_QPA_PLATFORMTHEME").is_none()
    {
        set_env("QT_QPA_PLATFORMTHEME", "xdgdesktopportal");
    }

    // Fusion follows the desktop's palette (light/dark) on every OS; the user
    // can still pick another style with QT_QUICK_CONTROLS_STYLE.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        set_env("QT_QUICK_CONTROLS_STYLE", "Fusion");
    }

    let icon = include_bytes!("../assets/uvpad-256.png");
    let app = qobject::uvpad_new_application(env!("CARGO_PKG_VERSION"), icon);
    if app.is_null() {
        anyhow::bail!("failed to start Qt");
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        // Packaged builds bring their own QML modules. Search them first: an
        // import path from the environment (another Qt's QML2_IMPORT_PATH, say)
        // would otherwise win, and that Qt's plugins fail to load with this one.
        for dir in bundled_qml_dirs() {
            engine
                .as_mut()
                .add_import_path(&QString::from(dir.to_string_lossy().as_ref()));
        }
        engine.load(&QUrl::from(&QString::from(
            "qrc:/qt/qml/uvpad/qml/main.qml",
        )));
    }
    let code = qobject::uvpad_exec();
    drop(engine); // destroys the Padder, and with it the preview files
    drop(app);
    if code != 0 {
        anyhow::bail!("Qt exited with code {code}");
    }
    Ok(())
}
