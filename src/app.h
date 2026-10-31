#pragma once

#include <QtWidgets/QApplication>
#include <memory>

#include "rust/cxx.h"

// A QApplication rather than cxx-qt-lib's QGuiApplication: platform themes
// only give their native file dialogs (KDE's, for one) to widget applications.
std::unique_ptr<QApplication> uvpad_new_application(rust::Str version);

int uvpad_exec();
