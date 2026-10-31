//! Qt Quick UI, started with `uvpad --gui`.
//!
//! The window (qml/main.qml) talks to one `Padder` object. Padding runs on a
//! background thread and writes a temporary preview PNG, which the window
//! shows. Saving copies that file (or re-encodes it with the chosen
//! compression) to wherever the user picks in the file dialog.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

use crate::{default_output, run, Level};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy)]
        #[qproperty(QString, status)]
        #[qproperty(bool, failed)]
        #[qproperty(QUrl, input_url, cxx_name = "inputUrl")]
        #[qproperty(QUrl, output_url, cxx_name = "outputUrl")]
        #[qproperty(QUrl, suggested_url, cxx_name = "suggestedUrl")]
        #[qproperty(QUrl, input_folder, cxx_name = "inputFolder")]
        #[qproperty(bool, slower)]
        #[qproperty(i32, compression)]
        type Padder = super::PadderRust;

        /// Pads the PNG at `url` and shows the result.
        #[qinvokable]
        fn process(self: Pin<&mut Padder>, url: &QUrl);

        /// Writes the shown result to `url`.
        #[qinvokable]
        fn save(self: Pin<&mut Padder>, url: &QUrl);

        /// The image given on the command line, if any (only returned once).
        #[qinvokable]
        #[cxx_name = "takeStartupUrl"]
        fn take_startup_url(self: &Padder) -> QUrl;
    }

    impl cxx_qt::Threading for Padder {}
}

/// Image passed as `uvpad --gui <INPUT>`.
static STARTUP: OnceLock<std::sync::Mutex<Option<PathBuf>>> = OnceLock::new();

/// The padded preview of one input, kept in the temp directory until replaced.
struct Preview {
    input: PathBuf,
    file: PathBuf,
    slower: bool,
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

#[derive(Default)]
pub struct PadderRust {
    busy: bool,
    status: QString,
    failed: bool,
    input_url: QUrl,
    output_url: QUrl,
    suggested_url: QUrl,
    input_folder: QUrl,
    slower: bool,
    /// 0 fast, 1 default, 2 best (the order of the combo box)
    compression: i32,

    preview: Option<Preview>,
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

fn preview_file(job: u64) -> PathBuf {
    std::env::temp_dir().join(format!("uvpad-preview-{}-{job}.png", std::process::id()))
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

        self.as_mut().set_input_url(to_url(&input));
        if let Some(dir) = input.parent() {
            self.as_mut().set_input_folder(to_url(dir));
        }
        self.as_mut().set_suggested_url(to_url(&default_output(&input)));
        self.as_mut().report(true, false, format!("Padding {}…", file_name(&input)));

        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let file = preview_file(job);
            let start = Instant::now();
            // The preview only lives in the temp directory, so encode it fast.
            let result = run(&input, &file, slower, Level::Fast);
            let elapsed = start.elapsed();
            let preview = Preview {
                input,
                file,
                slower,
            };
            let _ = thread.queue(move |mut padder| {
                if padder.rust().job != job {
                    return; // superseded; `preview` is dropped, which deletes its file
                }
                match result {
                    Ok(()) => {
                        let status = format!(
                            "Padded {} in {elapsed:.2?}{}",
                            file_name(&preview.input),
                            if slower { " (GIMP UVPad)" } else { "" },
                        );
                        padder.as_mut().set_output_url(to_url(&preview.file));
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
        let Some(preview) = &self.rust().preview else {
            self.report(false, true, "Nothing to save yet".into());
            return;
        };
        let (input, file, slower) = (preview.input.clone(), preview.file.clone(), preview.slower);
        let level = level(*self.compression());
        let job = self.as_mut().next_job();
        self.as_mut().report(true, false, format!("Saving {}…", file_name(&dest)));

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

    pub fn take_startup_url(&self) -> QUrl {
        STARTUP
            .get()
            .and_then(|m| m.lock().ok()?.take())
            .map(|p| to_url(&p))
            .unwrap_or_default()
    }
}

pub fn launch(input: Option<PathBuf>) -> Result<()> {
    let input = input
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .filter(|p| !p.as_os_str().is_empty());
    let _ = STARTUP.set(std::sync::Mutex::new(input));

    // Fusion follows the desktop's palette (light/dark) on every OS; the user
    // can still pick another style with QT_QUICK_CONTROLS_STYLE.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
    }

    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("uvpad"));
        app.as_mut().set_application_display_name(&QString::from("uvpad"));
        app.set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(&QString::from("qrc:/qt/qml/uvpad/qml/main.qml")));
    }
    let code = app.as_mut().map_or(1, |app| app.exec());
    drop(engine); // destroys the Padder, and with it the preview file
    if code != 0 {
        anyhow::bail!("Qt exited with code {code}");
    }
    Ok(())
}
