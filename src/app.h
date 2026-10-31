// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QtWidgets/QApplication>
#include <cstdint>
#include <memory>

#include "rust/cxx.h"

// A QApplication rather than cxx-qt-lib's QGuiApplication: platform themes
// only give their native file dialogs (KDE's, for one) to widget applications.
std::unique_ptr<QApplication> uvpad_new_application(rust::Str version,
                                                    rust::Slice<const std::uint8_t> icon_png);

int uvpad_exec();

// Sets an environment variable where Qt reads it. On Windows that is the C
// runtime's copy, which Rust's std::env::set_var does not update.
void uvpad_set_env(rust::Str name, rust::Str value);

// Puts a PNG file's bytes on the clipboard as image/png, plus the decoded
// image if `with_image` (some programs, like Paint, only take bitmaps).
void uvpad_copy_png(rust::Slice<const std::uint8_t> png, bool with_image);
