fn main() {
    // The Qt UI is optional, so a plain `cargo build` needs no Qt installation.
    #[cfg(feature = "gui")]
    {
        use cxx_qt_build::{CxxQtBuilder, QmlModule};

        CxxQtBuilder::new_qml_module(QmlModule::new("uvpad").qml_file("qml/main.qml"))
            .qt_module("Quick")
            .file("src/gui.rs")
            .build();
    }
}
