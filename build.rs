// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    // The Qt UI is optional, so a plain `cargo build` needs no Qt installation.
    #[cfg(feature = "gui")]
    {
        use cxx_qt_build::{CxxQtBuilder, QmlModule};

        CxxQtBuilder::new_qml_module(QmlModule::new("uvpad").qml_file("qml/main.qml"))
            .qt_module("Quick")
            // QApplication, for the desktop's native file dialogs (see src/app.h).
            .qt_module("Widgets")
            .file("src/gui.rs")
            .cpp_file("src/app.cpp")
            .build();
    }

    // The exe's icon. The resource script names the icon by absolute path,
    // so it does not matter where the resource compiler resolves paths from.
    #[cfg(windows)]
    {
        let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/uvpad.ico");
        let icon = icon.to_str().expect("non-UTF-8 path").replace('\\', "\\\\");
        let rc = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("uvpad.rc");
        std::fs::write(&rc, format!("1 ICON \"{icon}\"\n")).expect("failed to write uvpad.rc");
        println!("cargo::rerun-if-changed=assets/uvpad.ico");
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed the icon");
    }
}
