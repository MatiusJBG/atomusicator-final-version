#include "flutter_window.h"

#include <optional>
#include <shellapi.h>
#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>

namespace {
// File copies from Explorer use CF_HDROP, rather than the text clipboard.
class ClipboardScope {
 public:
  explicit ClipboardScope(HWND owner) : opened_(OpenClipboard(owner) != FALSE) {}
  ~ClipboardScope() { if (opened_) CloseClipboard(); }
  bool opened() const { return opened_; }
 private:
  bool opened_;
};
}  // namespace

#include "flutter/generated_plugin_registrant.h"

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  flutter::MethodChannel<flutter::EncodableValue> import_channel(
      flutter_controller_->engine()->messenger(), "atomusicator/audio_import",
      &flutter::StandardMethodCodec::GetInstance());
  import_channel.SetMethodCallHandler(
      [this](const auto& call, auto result) {
        if (call.method_name() != "readFilePaths") {
          result->NotImplemented();
          return;
        }
        ClipboardScope clipboard(GetHandle());
        if (!clipboard.opened()) {
          result->Error("clipboard_busy", "The clipboard is currently unavailable. Try again.");
          return;
        }
        flutter::EncodableList paths;
        if (IsClipboardFormatAvailable(CF_HDROP)) {
          auto drop = static_cast<HDROP>(GetClipboardData(CF_HDROP));
          if (!drop) {
            result->Error("clipboard_read", "Could not read copied file paths.");
            return;
          }
          const UINT count = DragQueryFileW(drop, 0xFFFFFFFF, nullptr, 0);
          for (UINT i = 0; i < count; ++i) {
            const UINT length = DragQueryFileW(drop, i, nullptr, 0);
            std::wstring path(length + 1, L'\0');
            if (DragQueryFileW(drop, i, path.data(), length + 1) != length || length == 0) continue;
            const int bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
                path.data(), static_cast<int>(length), nullptr, 0, nullptr, nullptr);
            if (bytes <= 0) continue;
            std::string utf8(bytes, '\0');
            WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, path.data(),
                static_cast<int>(length), utf8.data(), bytes, nullptr, nullptr);
            paths.emplace_back(utf8);
          }
        }
        result->Success(flutter::EncodableValue(paths));
      });

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
