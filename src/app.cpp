#include "uvpad/src/app.h"

namespace {
// Qt keeps referring to these for as long as the application exists. The real
// command line is handled by clap, so Qt only gets the program name.
int argc = 1;
char name[] = "uvpad";
char *argv[] = {name, nullptr};
} // namespace

std::unique_ptr<QApplication> uvpad_new_application(rust::Str version)
{
    auto app = std::make_unique<QApplication>(argc, argv);
    QApplication::setApplicationName(QStringLiteral("uvpad"));
    QApplication::setApplicationDisplayName(QStringLiteral("uvpad"));
    QApplication::setApplicationVersion(QString::fromUtf8(version.data(), version.size()));
    return app;
}

int uvpad_exec()
{
    return QApplication::exec();
}
