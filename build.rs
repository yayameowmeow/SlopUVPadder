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
}
