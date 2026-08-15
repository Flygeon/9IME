#pragma once
//
// SogouSkin.h - Sogou (.ssf) input method skin support for 9IME
//
// A Sogou skin file (.ssf) is either:
//   1. a plain ZIP archive containing skin.ini and image assets, or
//   2. Sogou's proprietary AES-256-CBC encrypted container (starts with the
//      magic bytes "Skin"), whose plaintext is a custom archive of UTF-16
//      named entries. Both formats are supported here.
//
// Layout semantics (all coordinates are pixels at 96 DPI, scale with DPI):
//   [Scheme_H1]/[Scheme_V1]  candidate-window schemes
//     layout_horizontal = stretch,left,right   9-grid stretch margins
//     layout_vertical    = stretch,top,bottom
//     pinyin_marge       = top,gap,left,right  pinyin (preedit) area
//     zhongwen_marge     = gap,bottom,left,right  candidate area
//     separator          = color,x,y           separator line
//   [Scheme_H2]/[Scheme_V2]  extra images for pinyin/candidate highlight
//   [StatusBar]              floating status bar (bar.png + state icons)
//   [Display]                font and colors (colors are stored BGR)
//
// The pinyin row is placed at pinyin_marge[0] from the image top; the
// candidate block starts below it with the gap
//   pinyin_marge[1] + separator + zhongwen_marge[0]
// and its bottom edge sits zhongwen_marge[1] above the image bottom.
//

#include <windows.h>
#include <gdiplus.h>
#include <map>
#include <string>
#include <vector>

#pragma comment(lib, "gdiplus.lib")

namespace weasel {

class SogouSkin {
 public:
  SogouSkin();
  ~SogouSkin();

  // Load a .ssf file (zip or encrypted container). dpi is used to scale
  // layout values and drawing. Returns true when the skin becomes usable.
  bool Load(const std::wstring& path, UINT dpi);
  void Unload();
  bool Loaded() const { return loaded_; }
  UINT dpi() const { return dpi_; }

  // scale a 96-dpi value to the current dpi
  int S(int v) const;

  // ---- metadata (96 dpi values; font_size in pixels @96dpi) ----
  const std::wstring& name() const { return name_; }
  const std::wstring& font_name() const { return font_name_; }
  int font_size() const { return font_size_; }

  // colors as COLORREF (alpha 0xff)
  COLORREF preedit_color() const { return preedit_color_; }
  COLORREF hilited_candidate_color() const { return hilited_candidate_color_; }
  COLORREF candidate_color() const { return candidate_color_; }
  COLORREF fallback_back_color() const { return back_color_; }

  // ---- candidate-window scheme (96 dpi values) ----
  struct Scheme {
    Gdiplus::Bitmap* pic = nullptr;  // window background
    int img_w = 0, img_h = 0;        // native image size (96 dpi)
    // 9-grid stretch margins
    int stretch_left = 0, stretch_right = 0, stretch_top = 0,
        stretch_bottom = 0;
    // pinyin (preedit) area: left..right, top (bottom = top + line height)
    int preedit_left = 0, preedit_top = 0, preedit_right = 0;
    // candidate area
    int candidate_left = 0, candidate_right = 0, candidate_bottom = 0;
    int gap = 0;  // pinyin -> candidates gap (incl. separator)
    COLORREF separator_color = 0xffffffff;  // transparent when absent
    Gdiplus::Bitmap* candidate_highlight = nullptr;  // zhongwen_pic
    Gdiplus::Bitmap* preedit_highlight = nullptr;    // pinyin_pic
  };

  const Scheme& horizontal() const { return h_; }
  const Scheme& vertical() const { return v_; }
  bool HasScheme(bool vertical) const {
    return vertical ? v_.pic != nullptr : h_.pic != nullptr;
  }

  // ---- status bar ----
  struct StatusButton {
    std::wstring id;    // cn_en / quan_ban / biaodian / ...
    POINT pos = {0, 0};  // 96 dpi
    bool display = false;
    std::vector<Gdiplus::Bitmap*> normal, down, hover;
  };
  Gdiplus::Bitmap* status_bar_pic() const { return bar_pic_; }
  const std::vector<StatusButton>& status_buttons() const { return buttons_; }
  int status_bar_w() const { return bar_w_; }
  int status_bar_h() const { return bar_h_; }

  // ---- drawing helpers (GDI+ must be initialized, dc is a memory DC) ----
  // Stretch the window background image onto the window rect using the
  // 9-grid defined by the scheme's stretch margins.
  void DrawBackground(HDC dc, const CRect& rc, const Scheme& scheme) const;
  // Draw the pinyin/candidate separator line defined by the scheme.
  // preedit_line_h is the preedit line height in 96-dpi pixels.
  void DrawSeparator(HDC dc, const CRect& rc, const Scheme& scheme,
                     int preedit_line_h) const;
  // Stretch candidate_highlight (or preedit_highlight) over a rect.
  void DrawHighlight(HDC dc, const CRect& rc, Gdiplus::Bitmap* img) const;
  // Draw the floating status bar background + state icons onto rc.
  void DrawStatusBar(HDC dc, const CRect& rc, bool ascii_mode,
                     bool full_shape, bool disabled) const;
  // Draw the cn/en state icon at rc (used inside the candidate window).
  void DrawCnEnIcon(HDC dc, const CRect& rc, bool ascii_mode,
                    bool disabled) const;

 private:
  bool Extract(const std::wstring& path);
  bool ParseSkinIni();
  bool DecodeImages();
  bool ParseScheme(const std::wstring& section, Scheme& scheme,
                   const std::wstring& suffix);
  Gdiplus::Bitmap* LoadBitmap(const std::wstring& filename);
  std::wstring FindFile(const std::wstring& filename) const;
  std::vector<BYTE>* FindFileData(const std::wstring& filename);
  int GetInt(const std::wstring& section, const std::wstring& key,
             int defval) const;
  std::wstring GetString(const std::wstring& section,
                         const std::wstring& key) const;
  COLORREF GetColor(const std::wstring& section, const std::wstring& key,
                    COLORREF defval) const;
  void ParseIniInto(const std::vector<BYTE>& raw);
  void ClearImages();

  bool loaded_ = false;
  UINT dpi_ = 96;
  std::wstring name_, font_name_;
  int font_size_ = 12;
  COLORREF preedit_color_ = RGB(0x48, 0x48, 0x48);
  COLORREF hilited_candidate_color_ = RGB(0xff, 0xbd, 0x35);
  COLORREF candidate_color_ = RGB(0x4b, 0x4b, 0x4b);
  COLORREF back_color_ = RGB(0xff, 0xff, 0xff);

  Scheme h_, v_;
  Gdiplus::Bitmap* bar_pic_ = nullptr;
  std::vector<StatusButton> buttons_;
  int bar_w_ = 0, bar_h_ = 0;

  // extracted files, keyed by lowercase file name
  std::map<std::wstring, std::vector<BYTE>> files_;
  // parsed skin.ini sections (lowercase section -> key -> value)
  std::map<std::wstring, std::map<std::wstring, std::wstring>> ini_;
};

}  // namespace weasel
