// SogouSkin.cpp - Sogou (.ssf) skin loading and rendering (9IME)
#include "stdafx.h"
#include "SogouSkin.h"

#include <bcrypt.h>
#include <algorithm>
#include <fstream>
#include <objidl.h>

#include <WeaselUtility.h>

#include "miniz.h"

#pragma comment(lib, "bcrypt.lib")

namespace weasel {

namespace {

// helpers ---------------------------------------------------------------

std::wstring ToLower(const std::wstring& s) {
  std::wstring r(s);
  std::transform(r.begin(), r.end(), r.begin(), ::towlower);
  return r;
}

std::wstring Trim(const std::wstring& s) {
  size_t b = s.find_first_not_of(L" \t\r\n");
  if (b == std::wstring::npos)
    return L"";
  size_t e = s.find_last_not_of(L" \t\r\n");
  return s.substr(b, e - b + 1);
}

std::wstring Utf8ToW(const std::string& s) {
  if (s.empty())
    return L"";
  int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), NULL, 0);
  std::wstring r(n, 0);
  MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), &r[0], n);
  return r;
}

std::vector<std::wstring> Split(const std::wstring& s, wchar_t sep) {
  std::vector<std::wstring> parts;
  size_t start = 0;
  for (size_t i = 0; i <= s.size(); ++i) {
    if (i == s.size() || s[i] == sep) {
      parts.push_back(s.substr(start, i - start));
      start = i + 1;
    }
  }
  return parts;
}

int ParseInt(const std::wstring& s, int defval) {
  std::wstring t = Trim(s);
  if (t.empty())
    return defval;
  int base = 10;
  if (t.size() > 2 && t[0] == L'0' && (t[1] == L'x' || t[1] == L'X')) {
    base = 16;
    t = t.substr(2);
  }
  wchar_t* end = NULL;
  long v = wcstol(t.c_str(), &end, base);
  if (end == t.c_str())
    return defval;
  return (int)v;
}

std::vector<int> ParseIntList(const std::wstring& s) {
  std::vector<int> out;
  for (auto& p : Split(s, L','))
    out.push_back(ParseInt(p, 0));
  return out;
}

// Sogou stores colors in BGR byte order inside the 0xRRGGBB notation.
COLORREF ParseColor(int bgr) {
  return RGB(bgr & 0xff, (bgr >> 8) & 0xff, (bgr >> 16) & 0xff);
}

uint32_t ReadU32LE(const BYTE* p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) |
         ((uint32_t)p[3] << 24);
}

// zlib inflate with growing output buffer
bool ZlibInflate(const BYTE* src, size_t src_len, std::vector<BYTE>& out) {
  size_t cap = 1u << 20;  // 1 MiB
  while (cap <= (1u << 26)) {
    out.resize(cap);
    mz_ulong out_len = (mz_ulong)cap;
    int st = mz_uncompress(out.data(), &out_len, src, (mz_ulong)src_len);
    if (st == MZ_OK) {
      out.resize(out_len);
      return true;
    }
    if (st != MZ_BUF_ERROR)
      return false;
    cap <<= 1;
  }
  return false;
}

// Sogou's AES-256-CBC key/iv for encrypted skins (public knowledge, from
// the widely used ssfconv converter).
const BYTE kSsfKey[32] = {0x52, 0x36, 0x46, 0x1A, 0xD3, 0x85, 0x03, 0x66,
                          0x90, 0x45, 0x16, 0x28, 0x79, 0x03, 0x36, 0x23,
                          0xDD, 0xBE, 0x6F, 0x03, 0xFF, 0x04, 0xE3, 0xCA,
                          0xD5, 0x7F, 0xFC, 0xA3, 0x50, 0xE4, 0x9E, 0xD9};
const BYTE kSsfIv[16] = {0xE0, 0x7A, 0xAD, 0x35, 0xE0, 0x90, 0xAA, 0x03,
                         0x8A, 0x51, 0xFD, 0x05, 0xDF, 0x8C, 0x5D, 0x0F};

bool Aes256CbcDecrypt(const std::vector<BYTE>& in, std::vector<BYTE>& out) {
  if (in.empty() || (in.size() % 16) != 0)
    return false;
  BCRYPT_ALG_HANDLE h_alg = NULL;
  if (BCryptOpenAlgorithmProvider(&h_alg, BCRYPT_AES_ALGORITHM, NULL, 0) != 0)
    return false;
  BCryptSetProperty(h_alg, BCRYPT_CHAINING_MODE,
                    (PUCHAR)BCRYPT_CHAIN_MODE_CBC,
                    sizeof(BCRYPT_CHAIN_MODE_CBC), 0);
  BCRYPT_KEY_HANDLE h_key = NULL;
  bool ok = false;
  if (BCryptGenerateSymmetricKey(h_alg, &h_key, NULL, 0, (PUCHAR)kSsfKey,
                                 sizeof(kSsfKey), 0) == 0) {
    out.resize(in.size());
    ULONG done = 0;
    BYTE iv[16];
    memcpy(iv, kSsfIv, sizeof(iv));
    if (BCryptDecrypt(h_key, (PUCHAR)in.data(), (ULONG)in.size(), NULL, iv,
                      sizeof(iv), out.data(), (ULONG)out.size(), &done,
                      0) == 0)
      ok = true;
    BCryptDestroyKey(h_key);
  }
  BCryptCloseAlgorithmProvider(h_alg, 0);
  return ok;
}

}  // namespace

void SogouSkin::Log(const std::wstring& msg) {
  std::wofstream out(WeaselLogPath() / L"sogou-skin.log", std::ios::app);
  if (out)
    out << msg << std::endl;
}

SogouSkin::SogouSkin() {}

SogouSkin::~SogouSkin() {
  Unload();
}

void SogouSkin::ClearImages() {
  auto clear = [](Gdiplus::Bitmap*& p) {
    if (p) {
      delete p;
      p = nullptr;
    }
  };
  clear(h_.pic);
  clear(h_.candidate_highlight);
  clear(h_.preedit_highlight);
  clear(v_.pic);
  clear(v_.candidate_highlight);
  clear(v_.preedit_highlight);
  clear(bar_pic_);
  for (auto& b : buttons_) {
    for (auto* p : b.normal)
      delete p;
    for (auto* p : b.down)
      delete p;
    for (auto* p : b.hover)
      delete p;
    b.normal.clear();
    b.down.clear();
    b.hover.clear();
  }
  buttons_.clear();
}

void SogouSkin::Unload() {
  loaded_ = false;
  ClearImages();
  files_.clear();
  ini_.clear();
  h_ = Scheme();
  v_ = Scheme();
  bar_pic_ = nullptr;
  bar_w_ = bar_h_ = 0;
  name_.clear();
  font_name_.clear();
}

int SogouSkin::S(int v) const {
  return MulDiv(v, (int)dpi_, 96);
}

// --- file extraction ----------------------------------------------------

bool SogouSkin::Extract(const std::wstring& path) {
  HANDLE h_file =
      CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL,
                  OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
  if (h_file == INVALID_HANDLE_VALUE) {
    Log(L"[skin] cannot open file, error " +
        std::to_wstring(GetLastError()));
    return false;
  }
  LARGE_INTEGER sz = {0};
  if (!GetFileSizeEx(h_file, &sz) || sz.QuadPart <= 0 ||
      sz.QuadPart > (64 * 1024 * 1024)) {
    CloseHandle(h_file);
    return false;
  }
  std::vector<BYTE> buf((size_t)sz.QuadPart);
  DWORD read = 0;
  BOOL ok = ReadFile(h_file, buf.data(), (DWORD)buf.size(), &read, NULL);
  CloseHandle(h_file);
  if (!ok || read != buf.size())
    return false;

  if (buf.size() >= 8 && buf[0] == 'S' && buf[1] == 'k' && buf[2] == 'i' &&
      buf[3] == 'n') {
    // Sogou encrypted container: "Skin" magic, then AES-256-CBC stream
    // whose payload [4..] is a zlib stream of a custom archive.
    std::vector<BYTE> cipher(buf.begin() + 8, buf.end());
    std::vector<BYTE> plain;
    if (!Aes256CbcDecrypt(cipher, plain) || plain.size() < 8)
      return false;
    std::vector<BYTE> data;
    if (!ZlibInflate(plain.data() + 4, plain.size() - 4, data) ||
        data.size() < 8)
      return false;
    uint32_t offsets_size = ReadU32LE(data.data() + 4);
    if (offsets_size > data.size() - 8)
      return false;
    for (uint32_t off = 8; off + 4 <= 8 + offsets_size; off += 4) {
      uint32_t e = ReadU32LE(data.data() + off);
      if (e + 8 > data.size())
        continue;
      uint32_t name_len = ReadU32LE(data.data() + e);
      if (name_len > data.size() - e - 4)
        continue;
      std::wstring name((wchar_t*)(data.data() + e + 4), name_len / 2);
      uint32_t content_len = ReadU32LE(data.data() + e + 4 + name_len);
      if (e + 8 + name_len + content_len > data.size())
        continue;
      std::vector<BYTE> content(
          data.begin() + e + 8 + name_len,
          data.begin() + e + 8 + name_len + content_len);
      files_[ToLower(name)] = content;
    }
    return !files_.empty();
  }

  // plain zip
  mz_zip_archive zip;
  memset(&zip, 0, sizeof(zip));
  if (!mz_zip_reader_init_mem(&zip, buf.data(), buf.size(), 0)) {
    Log(L"[skin] not a valid zip archive");
    return false;
  }
  mz_uint n = mz_zip_reader_get_num_files(&zip);
  for (mz_uint i = 0; i < n; ++i) {
    mz_zip_archive_file_stat st;
    if (!mz_zip_reader_file_stat(&zip, i, &st))
      continue;
    if (st.m_uncomp_size == 0 || st.m_uncomp_size > (64 * 1024 * 1024))
      continue;
    size_t size = 0;
    void* p = mz_zip_reader_extract_to_heap(&zip, i, &size, 0);
    if (!p)
      continue;
    std::wstring name = Utf8ToW(st.m_filename);
    std::vector<BYTE> content((BYTE*)p, (BYTE*)p + size);
    files_[ToLower(name)] = content;
    mz_free(p);
  }
  mz_zip_reader_end(&zip);
  return !files_.empty();
}

std::wstring SogouSkin::FindFile(const std::wstring& filename) const {
  auto it = files_.find(ToLower(filename));
  if (it != files_.end())
    return it->first;
  return L"";
}

std::vector<BYTE>* SogouSkin::FindFileData(const std::wstring& filename) {
  auto it = files_.find(ToLower(filename));
  if (it != files_.end())
    return &it->second;
  return NULL;
}

Gdiplus::Bitmap* SogouSkin::LoadBitmap(const std::wstring& filename) {
  if (filename.empty())
    return NULL;
  std::vector<BYTE>* data = FindFileData(filename);
  if (!data || data->empty())
    return NULL;
  HGLOBAL hg = GlobalAlloc(GMEM_MOVEABLE, data->size());
  if (!hg)
    return NULL;
  void* p = GlobalLock(hg);
  memcpy(p, data->data(), data->size());
  GlobalUnlock(hg);
  IStream* stream = NULL;
  if (FAILED(CreateStreamOnHGlobal(hg, TRUE, &stream))) {
    GlobalFree(hg);
    return NULL;
  }
  Gdiplus::Bitmap* bmp = Gdiplus::Bitmap::FromStream(stream);
  stream->Release();
  return bmp;
}

// --- skin.ini parsing ---------------------------------------------------

void SogouSkin::ParseIniInto(const std::vector<BYTE>& raw) {
  // decode: UTF-16LE BOM, UTF-8 BOM, strict UTF-8, else GBK (CP936)
  std::wstring text;
  if (raw.size() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE) {
    text.assign((wchar_t*)(raw.data() + 2), (raw.size() - 2) / 2);
  } else if (raw.size() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB &&
             raw[2] == 0xBF) {
    text = Utf8ToW(std::string((char*)raw.data() + 3, raw.size() - 3));
  } else {
    std::string s((char*)raw.data(), raw.size());
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.c_str(),
                                (int)s.size(), NULL, 0);
    if (n > 0) {
      text.resize(n);
      MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), &text[0], n);
    } else {
      n = MultiByteToWideChar(936, 0, s.c_str(), (int)s.size(), NULL, 0);
      if (n > 0) {
        text.resize(n);
        MultiByteToWideChar(936, 0, s.c_str(), (int)s.size(), &text[0], n);
      }
    }
  }

  ini_.clear();
  std::wstring section;
  std::vector<std::wstring> lines = Split(text, L'\n');
  for (auto& raw_line : lines) {
    std::wstring line = Trim(raw_line);
    if (line.empty() || line[0] == L';' || line[0] == L'#')
      continue;
    if (line[0] == L'[') {
      size_t close = line.find(L']');
      if (close != std::wstring::npos)
        section = ToLower(Trim(line.substr(1, close - 1)));
      continue;
    }
    if (section.empty())
      continue;
    size_t eq = line.find(L'=');
    std::wstring key, value;
    if (eq == std::wstring::npos) {
      size_t sp = line.find_first_of(L" \t");
      if (sp == std::wstring::npos) {
        key = line;
        value = L"1";  // bare key, treat as enabled
      } else {
        key = line.substr(0, sp);
        value = line.substr(sp + 1);
      }
    } else {
      key = line.substr(0, eq);
      value = line.substr(eq + 1);
    }
    ini_[section][ToLower(Trim(key))] = Trim(value);
  }
}

int SogouSkin::GetInt(const std::wstring& section,
                      const std::wstring& key,
                      int defval) const {
  auto s = ini_.find(ToLower(section));
  if (s == ini_.end())
    return defval;
  auto k = s->second.find(ToLower(key));
  if (k == s->second.end())
    return defval;
  return ParseInt(k->second, defval);
}

std::wstring SogouSkin::GetString(const std::wstring& section,
                                  const std::wstring& key) const {
  auto s = ini_.find(ToLower(section));
  if (s == ini_.end())
    return L"";
  auto k = s->second.find(ToLower(key));
  if (k == s->second.end())
    return L"";
  return k->second;
}

COLORREF SogouSkin::GetColor(const std::wstring& section,
                             const std::wstring& key,
                             COLORREF defval) const {
  std::wstring v = GetString(section, key);
  if (v.empty())
    return defval;
  return ParseColor(ParseInt(v, 0));
}

// --- scheme parsing -----------------------------------------------------

bool SogouSkin::ParseScheme(const std::wstring& section,
                            Scheme& scheme,
                            const std::wstring& suffix) {
  std::wstring pic_key = suffix.empty() ? L"pic" : suffix + L"_pic";
  scheme.pic = LoadBitmap(GetString(section, pic_key));
  if (!scheme.pic)
    return false;
  scheme.img_w = (int)scheme.pic->GetWidth();
  scheme.img_h = (int)scheme.pic->GetHeight();
  std::wstring lh_key =
      suffix.empty() ? L"layout_horizontal" : suffix + L"_layout_horizontal";
  std::wstring lv_key =
      suffix.empty() ? L"layout_vertical" : suffix + L"_layout_vertical";
  std::vector<int> lh = ParseIntList(GetString(section, lh_key));
  std::vector<int> lv = ParseIntList(GetString(section, lv_key));
  if (lh.size() >= 3 && lv.size() >= 3) {
    scheme.stretch_left = lh[1];
    scheme.stretch_right = lh[2];
    scheme.stretch_top = lv[1];
    scheme.stretch_bottom = lv[2];
  }
  std::vector<int> pm = ParseIntList(GetString(section, L"pinyin_marge"));
  std::vector<int> zm = ParseIntList(GetString(section, L"zhongwen_marge"));
  if (pm.size() >= 4 && zm.size() >= 4) {
    scheme.preedit_left = pm[2];
    scheme.preedit_top = pm[0];
    scheme.preedit_right = max(0, scheme.img_w - pm[3]);
    scheme.candidate_left = zm[2];
    scheme.candidate_right = max(0, scheme.img_w - zm[3]);
    scheme.candidate_bottom = max(0, scheme.img_h - zm[1]);
    scheme.gap = pm[1] + zm[0];
    std::vector<int> sep = ParseIntList(GetString(section, L"separator"));
    if (!sep.empty()) {
      scheme.separator_color = ParseColor(sep[0]);
      scheme.gap += 1;  // one pixel for the separator line
    }
  }
  return true;
}

bool SogouSkin::ParseSkinIni() {
  std::wstring ini_file = FindFile(L"skin.ini");
  if (ini_file.empty()) {
    Log(L"[skin] skin.ini not found in archive");
    return false;
  }
  std::vector<BYTE>* data = FindFileData(ini_file);
  if (!data)
    return false;
  ParseIniInto(*data);

  name_ = GetString(L"General", L"skin_name");
  if (GetString(L"General", L"skin_name").empty())
    name_ = L"Sogou skin";
  font_size_ = GetInt(L"Display", L"font_size", 12);
  if (font_size_ <= 0 || font_size_ > 96)
    font_size_ = 12;
  font_name_ = GetString(L"Display", L"font_ch");
  if (font_name_.empty())
    font_name_ = GetString(L"Display", L"font_en");
  preedit_color_ = GetColor(L"Display", L"pinyin_color", preedit_color_);
  hilited_candidate_color_ =
      GetColor(L"Display", L"zhongwen_first_color", hilited_candidate_color_);
  candidate_color_ = GetColor(L"Display", L"zhongwen_color", candidate_color_);

  // schemes
  ParseScheme(L"Scheme_H1", h_, L"");
  ParseScheme(L"Scheme_V1", v_, L"");
  // optional second-scheme highlight images
  h_.preedit_highlight = LoadBitmap(GetString(L"Scheme_H2", L"pinyin_pic"));
  h_.candidate_highlight = LoadBitmap(GetString(L"Scheme_H2", L"zhongwen_pic"));
  v_.preedit_highlight = LoadBitmap(GetString(L"Scheme_V2", L"pinyin_pic"));
  v_.candidate_highlight = LoadBitmap(GetString(L"Scheme_V2", L"zhongwen_pic"));

  // fallback background color: average color of the stretch region
  {
    Scheme* schemes[2] = {&h_, &v_};
    for (auto* sc : schemes) {
      if (!sc->pic)
        continue;
      int l = sc->stretch_left, t = sc->stretch_top;
      int w = sc->img_w - l - sc->stretch_right;
      int h = sc->img_h - t - sc->stretch_bottom;
      if (w <= 0 || h <= 0) {
        w = sc->img_w;
        h = sc->img_h;
        l = t = 0;
      }
      long long r = 0, g = 0, b = 0, cnt = 0;
      Gdiplus::Color c;
      for (int y = t; y < t + h; y += 4) {
        for (int x = l; x < l + w; x += 4) {
          if (sc->pic->GetPixel(x, y, &c) == Gdiplus::Ok && c.GetAlpha() > 0) {
            r += c.GetRed();
            g += c.GetGreen();
            b += c.GetBlue();
            cnt++;
          }
        }
      }
      if (cnt > 0) {
        back_color_ = RGB((BYTE)(r / cnt), (BYTE)(g / cnt), (BYTE)(b / cnt));
        break;
      }
    }
  }

  // status bar
  bar_pic_ = LoadBitmap(GetString(L"StatusBar", L"pic"));
  if (bar_pic_) {
    bar_w_ = (int)bar_pic_->GetWidth();
    bar_h_ = (int)bar_pic_->GetHeight();
  }
  static const wchar_t* kButtonIds[] = {
      L"cn_en",       L"biaodian",  L"quan_ban",   L"quan_shuang",
      L"fan_jian",    L"softkeyboard", L"menu",    L"sogousearch",
      L"passport",    L"skinmanager"};
  for (auto* id : kButtonIds) {
    std::wstring img_list = GetString(L"StatusBar", id);
    if (img_list.empty())
      continue;
    StatusButton b;
    b.id = id;
    auto load_list = [&](const std::wstring& list,
                         std::vector<Gdiplus::Bitmap*>& out) {
      for (auto& name : Split(list, L',')) {
        Gdiplus::Bitmap* img = LoadBitmap(name);
        if (img)
          out.push_back(img);
      }
    };
    load_list(img_list, b.normal);
    load_list(GetString(L"StatusBar", std::wstring(id) + L"_down"), b.down);
    load_list(GetString(L"StatusBar", std::wstring(id) + L"_hover"), b.hover);
    std::vector<int> pos = ParseIntList(GetString(L"StatusBar",
                                                  std::wstring(id) + L"_pos"));
    if (pos.size() >= 2) {
      b.pos.x = pos[0];
      b.pos.y = pos[1];
    }
    std::wstring disp = GetString(L"StatusBar",
                                  std::wstring(id) + L"_display");
    b.display = disp.empty() ? !b.normal.empty() : (disp == L"1");
    if (b.display)
      buttons_.push_back(b);
  }
  return true;
}

bool SogouSkin::Load(const std::wstring& path, UINT dpi) {
  Unload();
  if (path.empty()) {
    Log(L"[skin] Load: empty path");
    return false;
  }
  dpi_ = dpi > 0 ? dpi : 96;
  Log(L"[skin] Load: " + path + L" (dpi " + std::to_wstring(dpi_) + L")");
  if (!Extract(path)) {
    Log(L"[skin] Extract failed for: " + path);
    return false;
  }
  Log(L"[skin] Extract ok, " + std::to_wstring(files_.size()) +
      L" entries");
  if (!ParseSkinIni()) {
    Log(L"[skin] ParseSkinIni failed (skin.ini missing or no images)");
    return false;
  }
  if (!h_.pic && !v_.pic && !bar_pic_) {
    Log(L"[skin] no usable scheme images (H1/V1/StatusBar all missing)");
    return false;
  }
  loaded_ = true;
  Log(L"[skin] loaded ok: name=" + name_ + L" font=" + font_name_ +
      L" size=" + std::to_wstring(font_size_));
  return true;
}

// --- drawing ------------------------------------------------------------

void SogouSkin::DrawBackground(HDC dc,
                               const CRect& rc,
                               const Scheme& scheme) const {
  if (!scheme.pic || rc.IsRectEmpty())
    return;
  Gdiplus::Graphics g(dc);
  g.SetInterpolationMode(Gdiplus::InterpolationModeHighQualityBicubic);
  int src_l = scheme.stretch_left, src_r = scheme.stretch_right;
  int src_t = scheme.stretch_top, src_b = scheme.stretch_bottom;
  int sw = scheme.img_w, sh = scheme.img_h;
  int dl = S(src_l), dr = S(src_r), dt = S(src_t), db = S(src_b);
  int dw = rc.Width(), dh = rc.Height();
  if (dl + dr > dw) {
    float f = (float)dw / (float)(dl + dr);
    dl = (int)(dl * f);
    dr = (int)(dr * f);
    src_l = (int)(src_l * f);
    src_r = (int)(src_r * f);
  }
  if (dt + db > dh) {
    float f = (float)dh / (float)(dt + db);
    dt = (int)(dt * f);
    db = (int)(db * f);
    src_t = (int)(src_t * f);
    src_b = (int)(src_b * f);
  }
  auto draw = [&](int sx, int sy, int sw2, int sh2, int dx, int dy, int dw2,
                  int dh2) {
    if (dw2 <= 0 || dh2 <= 0 || sw2 <= 0 || sh2 <= 0)
      return;
    g.DrawImage(scheme.pic, Gdiplus::RectF((Gdiplus::REAL)dx, (Gdiplus::REAL)dy,
                                           (Gdiplus::REAL)dw2,
                                           (Gdiplus::REAL)dh2),
                (Gdiplus::REAL)sx, (Gdiplus::REAL)sy, (Gdiplus::REAL)sw2,
                (Gdiplus::REAL)sh2, Gdiplus::UnitPixel);
  };
  // corners
  draw(0, 0, src_l, src_t, rc.left, rc.top, dl, dt);
  draw(sw - src_r, 0, src_r, src_t, rc.right - dr, rc.top, dr, dt);
  draw(0, sh - src_b, src_l, src_b, rc.left, rc.bottom - db, dl, db);
  draw(sw - src_r, sh - src_b, src_r, src_b, rc.right - dr, rc.bottom - db,
       dr, db);
  // edges
  draw(src_l, 0, sw - src_l - src_r, src_t, rc.left + dl, rc.top, dw - dl - dr,
       dt);
  draw(src_l, sh - src_b, sw - src_l - src_r, src_b, rc.left + dl,
       rc.bottom - db, dw - dl - dr, db);
  draw(0, src_t, src_l, sh - src_t - src_b, rc.left, rc.top + dt, dl,
       dh - dt - db);
  draw(sw - src_r, src_t, src_r, sh - src_t - src_b, rc.right - dr,
       rc.top + dt, dr, dh - dt - db);
  // center
  draw(src_l, src_t, sw - src_l - src_r, sh - src_t - src_b, rc.left + dl,
       rc.top + dt, dw - dl - dr, dh - dt - db);
}

void SogouSkin::DrawSeparator(HDC dc,
                               const CRect& rc,
                               const Scheme& scheme,
                               int preedit_line_h) const {
  if (scheme.separator_color == 0xffffffff || rc.IsRectEmpty())
    return;
  Gdiplus::Graphics g(dc);
  Gdiplus::Pen pen(
      Gdiplus::Color(0xff, GetRValue(scheme.separator_color),
                     GetGValue(scheme.separator_color),
                     GetBValue(scheme.separator_color)),
      1.0f);
  int y = rc.top + S(scheme.preedit_top + preedit_line_h + scheme.gap - 1);
  int x1 = rc.left + S(scheme.candidate_left);
  int x2 = rc.left + S(scheme.candidate_right);
  if (x2 <= x1)
    x2 = rc.right - 1;
  g.DrawLine(&pen, (Gdiplus::REAL)x1, (Gdiplus::REAL)y, (Gdiplus::REAL)x2,
             (Gdiplus::REAL)y);
}

void SogouSkin::DrawHighlight(HDC dc,
                              const CRect& rc,
                              Gdiplus::Bitmap* img) const {
  if (!img || rc.IsRectEmpty())
    return;
  Gdiplus::Graphics g(dc);
  g.SetInterpolationMode(Gdiplus::InterpolationModeHighQualityBicubic);
  g.DrawImage(img, rc.left, rc.top, rc.Width(), rc.Height());
}

void SogouSkin::DrawCnEnIcon(HDC dc,
                              const CRect& rc,
                              bool ascii_mode,
                              bool disabled) const {
  if (rc.IsRectEmpty())
    return;
  for (const auto& b : buttons_) {
    if (b.id != L"cn_en" || b.normal.empty())
      continue;
    size_t idx = disabled ? 2 : (ascii_mode ? 1 : 0);
    Gdiplus::Bitmap* img =
        idx < b.normal.size() ? b.normal[idx] : b.normal[0];
    if (!img)
      return;
    Gdiplus::Graphics g(dc);
    g.SetInterpolationMode(Gdiplus::InterpolationModeHighQualityBicubic);
    int w = S((int)img->GetWidth());
    int h = S((int)img->GetHeight());
    int x = rc.left + (rc.Width() - w) / 2;
    int y = rc.top + (rc.Height() - h) / 2;
    g.DrawImage(img, x, y, w, h);
    return;
  }
}

void SogouSkin::DrawStatusBar(HDC dc,
                              const CRect& rc,
                              bool ascii_mode,
                              bool full_shape,
                              bool disabled) const {
  if (rc.IsRectEmpty())
    return;
  Gdiplus::Graphics g(dc);
  g.SetInterpolationMode(Gdiplus::InterpolationModeHighQualityBicubic);
  if (bar_pic_)
    g.DrawImage(bar_pic_, rc.left, rc.top, rc.Width(), rc.Height());
  for (const auto& b : buttons_) {
    Gdiplus::Bitmap* img = NULL;
    if (b.id == L"cn_en") {
      size_t idx = disabled ? 2 : (ascii_mode ? 1 : 0);
      img = idx < b.normal.size() ? b.normal[idx] : (b.normal.empty()
                                                         ? NULL
                                                         : b.normal[0]);
    } else if (b.id == L"quan_ban") {
      size_t idx = full_shape ? 0 : 1;
      img = idx < b.normal.size() ? b.normal[idx] : (b.normal.empty()
                                                         ? NULL
                                                         : b.normal[0]);
    } else {
      img = b.normal.empty() ? NULL : b.normal[0];
    }
    if (!img)
      continue;
    int w = S((int)img->GetWidth());
    int h = S((int)img->GetHeight());
    g.DrawImage(img, rc.left + S(b.pos.x), rc.top + S(b.pos.y), w, h);
  }
}

}  // namespace weasel
