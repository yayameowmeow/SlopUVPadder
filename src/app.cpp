// SPDX-License-Identifier: GPL-3.0-or-later

#include "uvpad/src/app.h"

#include <QtCore/QMimeData>
#include <QtGui/QClipboard>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QImageReader>
#include <QtGui/QPixmap>

namespace {
// Qt keeps referring to these for as long as the application exists. The real
// command line is handled by clap, so Qt only gets the program name.
int argc = 1;
char name[] = "uvpad";
char *argv[] = {name, nullptr};

QByteArray bytes(rust::Slice<const std::uint8_t> data)
{
    return QByteArray(reinterpret_cast<const char *>(data.data()), qsizetype(data.size()));
}
} // namespace

std::unique_ptr<QApplication> uvpad_new_application(rust::Str version,
                                                    rust::Slice<const std::uint8_t> icon_png)
{
    // Qt refuses to decode images over 256 MB by default, which a large
    // texture easily is; the previews are downscaled while decoding anyway.
    if (!qEnvironmentVariableIsSet("QT_IMAGEIO_MAXALLOC")) {
        QImageReader::setAllocationLimit(0);
    }

    auto app = std::make_unique<QApplication>(argc, argv);
    QApplication::setApplicationName(QStringLiteral("uvpad"));
    QApplication::setApplicationDisplayName(QStringLiteral("uvpad"));
    QApplication::setApplicationVersion(QString::fromUtf8(version.data(), version.size()));
    // Wayland compositors find the window's icon through the .desktop file.
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.yayameowmeow.uvpad"));

    QPixmap icon;
    if (icon.loadFromData(bytes(icon_png), "PNG")) {
        QApplication::setWindowIcon(QIcon(icon));
    }
    return app;
}

int uvpad_exec()
{
    return QApplication::exec();
}

void uvpad_set_env(rust::Str name, rust::Str value)
{
    qputenv(std::string(name).c_str(), QByteArray(value.data(), qsizetype(value.size())));
}

void uvpad_copy_png(rust::Slice<const std::uint8_t> png, bool with_image)
{
    const QByteArray data = bytes(png);
    auto *mime = new QMimeData;
    if (with_image) {
        QImage image;
        if (image.loadFromData(data, "PNG")) {
            mime->setImageData(image);
        }
    }
    mime->setData(QStringLiteral("image/png"), data);
    QGuiApplication::clipboard()->setMimeData(mime);
}
