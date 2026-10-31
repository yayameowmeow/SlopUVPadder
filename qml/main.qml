import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

import uvpad

ApplicationWindow {
    id: window
    width: 1100
    height: 780
    minimumWidth: 640
    minimumHeight: 420
    visible: true
    title: padder.inputUrl.toString() === "" ? "uvpad" : "uvpad — " + fileName(padder.inputUrl)

    // Which image is shown: the padded result (true) or the original (false).
    property bool showOutput: true
    readonly property bool hasInput: padder.inputUrl.toString() !== ""
    readonly property bool hasOutput: padder.outputUrl.toString() !== ""

    // Previews are decoded at most this large (longest side). Qt decodes the
    // whole PNG first, so a big texture costs 4 bytes per pixel while loading.
    readonly property int previewLimit: fullRes.checked ? 16384 : 4096
    readonly property bool downscaled: Math.max(padder.imageWidth, padder.imageHeight) > previewLimit
    readonly property Image shown: showOutput && hasOutput ? outputImage : inputImage

    function fileName(url) {
        const s = decodeURIComponent(url.toString())
        return s.substring(s.lastIndexOf("/") + 1)
    }

    function openImage(url) {
        showOutput = true
        view.resetZoom()
        padder.process(url)
    }

    Padder {
        id: padder
    }

    Component.onCompleted: {
        const url = padder.takeStartupUrl()
        if (url.toString() !== "")
            openImage(url)
    }

    FileDialog {
        id: openDialog
        title: "Open texture"
        fileMode: FileDialog.OpenFile
        nameFilters: [
            "Images (*.png *.jpg *.jpeg *.jxl *.webp *.tga *.tif *.tiff *.bmp *.gif *.exr *.hdr *.dds *.qoi *.ico *.pnm *.pbm *.pgm *.ppm *.pam *.ff *.avif)",
            "All files (*)"
        ]
        currentFolder: padder.inputFolder
        onAccepted: window.openImage(selectedFile)
    }

    FileDialog {
        id: saveDialog
        title: "Save padded texture"
        fileMode: FileDialog.SaveFile
        nameFilters: ["PNG images (*.png)"]
        defaultSuffix: "png"
        currentFolder: padder.inputFolder
        selectedFile: padder.suggestedUrl
        onAccepted: padder.save(selectedFile)
    }

    Shortcut {
        sequences: [StandardKey.Open]
        onActivated: openDialog.open()
    }
    Shortcut {
        sequences: [StandardKey.Save]
        enabled: saveButton.enabled
        onActivated: saveDialog.open()
    }
    Shortcut {
        sequence: "Space"
        enabled: window.hasOutput
        onActivated: window.showOutput = !window.showOutput
    }
    Shortcut {
        sequence: "0"
        onActivated: view.resetZoom()
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 6
            anchors.rightMargin: 6
            spacing: 6

            ToolButton {
                text: "Open…"
                onClicked: openDialog.open()
            }

            ToolSeparator {}

            Label { text: "Method" }
            ComboBox {
                id: method
                model: ["Nearest pixel", "GIMP UVPad (slower)"]
                implicitContentWidthPolicy: ComboBox.WidestText
                currentIndex: padder.slower ? 1 : 0
                onActivated: {
                    padder.slower = currentIndex === 1
                    if (window.hasInput)
                        padder.process(padder.inputUrl)
                }
            }

            Label { text: "Compression" }
            ComboBox {
                model: ["Fast", "Default", "Best"]
                implicitContentWidthPolicy: ComboBox.WidestText
                currentIndex: padder.compression
                onActivated: padder.compression = currentIndex
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: "PNG compression of the saved file: fast is quickest, best is smallest"
            }

            Item { Layout.fillWidth: true }

            TabBar {
                id: tabs
                enabled: window.hasOutput
                currentIndex: window.showOutput ? 1 : 0
                TabButton {
                    text: "Input"
                    width: implicitWidth
                    onClicked: window.showOutput = false
                }
                TabButton {
                    text: "Output"
                    width: implicitWidth
                    onClicked: window.showOutput = true
                }
            }

            ToolSeparator {}

            Button {
                id: saveButton
                text: "Save…"
                highlighted: true
                enabled: window.hasOutput && !padder.busy
                onClicked: saveDialog.open()
            }
        }
    }

    footer: Pane {
        padding: 6
        RowLayout {
            anchors.fill: parent
            spacing: 8
            BusyIndicator {
                running: padder.busy
                visible: running
                Layout.preferredWidth: 22
                Layout.preferredHeight: 22
            }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                color: padder.failed ? "#e5484d" : palette.windowText
                text: padder.status !== "" ? padder.status
                    : "Drop a texture anywhere in the window, or press Ctrl+O"
            }
            CheckBox {
                id: fullRes
                text: "Full-resolution preview"
                visible: Math.max(padder.imageWidth, padder.imageHeight) > 4096
                padding: 0
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: "Show large textures at full size (up to 16384 px) instead of downscaled.\n"
                              + "Needs about 4 bytes of memory per pixel for each of input and output."
            }
            Label {
                visible: padder.imageWidth > 0
                opacity: 0.7
                text: padder.imageWidth + " × " + padder.imageHeight
                      + (window.downscaled ? " (preview downscaled)" : "")
                      + "   " + Math.round(view.viewScale * 100) + "%"
            }
        }
    }

    // Checkerboard, so transparency in the input is visible.
    Canvas {
        id: checker
        anchors.fill: parent
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            const s = 12
            ctx.fillStyle = "#3a3a3a"
            ctx.fillRect(0, 0, width, height)
            ctx.fillStyle = "#4a4a4a"
            for (let y = 0; y < height; y += s)
                for (let x = ((y / s) % 2) * s; x < width; x += 2 * s)
                    ctx.fillRect(x, y, s, s)
        }
    }

    Flickable {
        id: view
        anchors.fill: parent
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        // Scale that fits the whole image, and the user's zoom on top of it.
        readonly property real fitScale: padder.imageWidth > 0
            ? Math.min(width / padder.imageWidth, height / padder.imageHeight, 1) : 1
        property real zoom: 1
        readonly property real viewScale: fitScale * zoom

        contentWidth: Math.max(width, padder.imageWidth * viewScale)
        contentHeight: Math.max(height, padder.imageHeight * viewScale)

        function resetZoom() {
            zoom = 1
            contentX = 0
            contentY = 0
        }

        // Zoom around point (x, y) of the view.
        function zoomBy(factor, x, y) {
            const next = Math.max(1, Math.min(zoom * factor, 256 / Math.max(fitScale, 1e-6)))
            const f = next / zoom
            const cx = (contentX + x) * f - x
            const cy = (contentY + y) * f - y
            zoom = next
            contentX = Math.max(0, Math.min(cx, contentWidth - width))
            contentY = Math.max(0, Math.min(cy, contentHeight - height))
        }

        // Both stay loaded, so flipping between them is instant and keeps the
        // zoom and position (they have the same size).
        PreviewImage {
            id: inputImage
            source: padder.inputPreviewUrl
            visible: !(window.showOutput && window.hasOutput)
        }
        PreviewImage {
            id: outputImage
            source: padder.outputUrl
            visible: window.showOutput && window.hasOutput
        }
    }

    component PreviewImage: Image {
        asynchronous: true
        cache: false
        retainWhileLoading: true
        fillMode: Image.Stretch
        sourceSize: Qt.size(window.previewLimit, window.previewLimit)
        // Show real pixels when zoomed in, so the padded edges can be inspected.
        smooth: view.viewScale < 2 || window.downscaled
        // Fixed: Qt cannot rebuild mipmaps for an already uploaded texture.
        mipmap: true
        anchors.centerIn: parent
        // Laid out at the texture's real size, whatever size it was decoded at.
        width: padder.imageWidth * view.viewScale
        height: padder.imageHeight * view.viewScale
    }

    // Zoom gestures, in view coordinates; dragging still pans the Flickable below.
    Item {
        anchors.fill: view
        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: (event) => {
                view.zoomBy(Math.pow(1.0015, event.angleDelta.y), point.position.x, point.position.y)
                event.accepted = true
            }
        }
        TapHandler {
            onDoubleTapped: view.resetZoom()
        }
        PinchHandler {
            target: null
            onScaleChanged: (delta) => view.zoomBy(delta, centroid.position.x, centroid.position.y)
        }
    }

    Label {
        anchors.centerIn: parent
        visible: !window.hasInput
        horizontalAlignment: Text.AlignHCenter
        font.pixelSize: 20
        opacity: 0.8
        text: "Drop a texture here\n\nor press Ctrl+O to browse"
    }

    BusyIndicator {
        anchors.centerIn: parent
        running: padder.busy || window.shown.status === Image.Loading
        visible: running
    }

    DropArea {
        id: drop
        anchors.fill: parent
        keys: ["text/uri-list"]
        onEntered: (drag) => drag.accept(Qt.CopyAction)
        onDropped: (drag) => {
            if (drag.hasUrls) {
                window.openImage(drag.urls[0])
                drag.accept(Qt.CopyAction)
            }
        }

        Rectangle {
            anchors.fill: parent
            anchors.margins: 8
            visible: drop.containsDrag
            color: Qt.alpha(palette.highlight, 0.15)
            border.color: palette.highlight
            border.width: 3
            radius: 8
            Label {
                anchors.centerIn: parent
                text: "Drop to pad"
                font.pixelSize: 24
            }
        }
    }
}
