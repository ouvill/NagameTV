#include "video_item.h"

#include "player_controller.h"

#include <QOpenGLContext>
#include <QOpenGLFramebufferObject>
#include <QOpenGLFramebufferObjectFormat>

#include <array>

namespace {
void *resolveGl(void *, const char *name) {
    auto *context = QOpenGLContext::currentContext();
    if (!context)
        return nullptr;
    return reinterpret_cast<void *>(context->getProcAddress(name));
}

class VideoRenderer final : public QQuickFramebufferObject::Renderer {
public:
    ~VideoRenderer() override {
        if (auto *controller = PlayerController::instance())
            if (controller->nativePlayer())
                mirakurun_player_free_renderer(controller->nativePlayer());
    }

    QOpenGLFramebufferObject *createFramebufferObject(const QSize &size) override {
        QOpenGLFramebufferObjectFormat format;
        format.setAttachment(QOpenGLFramebufferObject::CombinedDepthStencil);
        return new QOpenGLFramebufferObject(size, format);
    }

    void render() override {
        auto *controller = PlayerController::instance();
        auto *player = controller ? controller->nativePlayer() : nullptr;
        if (!player || !framebufferObject())
            return;

        if (!initialized_) {
            std::array<char, 512> error{};
            initialized_ = mirakurun_player_init_renderer(
                player, resolveGl, nullptr, error.data(), error.size());
        }
        if (initialized_) {
            const QSize size = framebufferObject()->size();
            mirakurun_player_render(player, framebufferObject()->handle(),
                                    size.width(), size.height());
        }
        update();
    }

private:
    bool initialized_ = false;
};
}

VideoItem::VideoItem(QQuickItem *parent) : QQuickFramebufferObject(parent) {
    setMirrorVertically(true);
}

QQuickFramebufferObject::Renderer *VideoItem::createRenderer() const {
    return new VideoRenderer;
}
