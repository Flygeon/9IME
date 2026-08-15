// SkinSettings.cpp - skin management for the 9IME deployer
#include "stdafx.h"
#include "SkinSettings.h"

#include <algorithm>
#include <filesystem>
#include <fstream>

#include <WeaselUtility.h>

namespace fs = std::filesystem;

namespace skin_settings {
namespace {

fs::path UserDir() {
  return WeaselUserDataPath();
}

std::wstring Trim(const std::wstring& s) {
  size_t b = s.find_first_not_of(L" \t\r\n");
  if (b == std::wstring::npos)
    return L"";
  size_t e = s.find_last_not_of(L" \t\r\n");
  return s.substr(b, e - b + 1);
}

bool IsIndented(const std::wstring& s) {
  return !s.empty() && (s[0] == L' ' || s[0] == L'\t');
}

// Decode a text file: UTF-8 (with or without BOM), fall back to GBK.
std::wstring DecodeBytes(const std::vector<char>& bytes) {
  const char* p = bytes.data();
  int len = (int)bytes.size();
  if (len >= 3 && (BYTE)bytes[0] == 0xEF && (BYTE)bytes[1] == 0xBB &&
      (BYTE)bytes[2] == 0xBF) {
    p += 3;
    len -= 3;
  }
  int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, p, len, NULL, 0);
  if (n > 0) {
    std::wstring s(n, 0);
    MultiByteToWideChar(CP_UTF8, 0, p, len, &s[0], n);
    return s;
  }
  n = MultiByteToWideChar(936, 0, bytes.data(), (int)bytes.size(), NULL, 0);
  if (n > 0) {
    std::wstring s(n, 0);
    MultiByteToWideChar(936, 0, bytes.data(), (int)bytes.size(), &s[0], n);
    return s;
  }
  return L"";
}

std::vector<char> EncodeUtf8(const std::wstring& s) {
  std::vector<char> out;
  int n = WideCharToMultiByte(CP_UTF8, 0, s.c_str(), (int)s.size(), NULL, 0,
                              NULL, NULL);
  if (n <= 0)
    return out;
  out.resize(n);
  WideCharToMultiByte(CP_UTF8, 0, s.c_str(), (int)s.size(), out.data(), n,
                      NULL, NULL);
  return out;
}

std::vector<std::wstring> ReadLines(const fs::path& p) {
  std::vector<std::wstring> lines;
  std::ifstream in(p, std::ios::binary);
  if (!in)
    return lines;
  std::vector<char> bytes((std::istreambuf_iterator<char>(in)),
                          std::istreambuf_iterator<char>());
  std::wstring text = DecodeBytes(bytes);
  std::wstring line;
  for (wchar_t ch : text) {
    if (ch == L'\n') {
      if (!line.empty() && line.back() == L'\r')
        line.pop_back();
      lines.push_back(line);
      line.clear();
    } else {
      line.push_back(ch);
    }
  }
  if (!line.empty())
    lines.push_back(line);
  return lines;
}

bool WriteLines(const fs::path& p, const std::vector<std::wstring>& lines) {
  std::wstring text;
  for (const auto& l : lines)
    text += l + L"\r\n";
  std::vector<char> bytes = EncodeUtf8(text);
  std::ofstream out(p, std::ios::binary | std::ios::trunc);
  if (!out)
    return false;
  out.write(bytes.data(), bytes.size());
  return out.good();
}

// Extract the value of "key" from a config line like "  key: value".
std::wstring ValueOf(const std::wstring& line, const std::wstring& key) {
  std::wstring t = Trim(line);
  if (t.rfind(key + L":", 0) != 0)
    return L"";
  std::wstring v = Trim(t.substr(key.size() + 1));
  if (v.size() >= 2 && v.front() == L'\"' && v.back() == L'\"')
    v = v.substr(1, v.size() - 2);
  return v;
}

// Find the first line index of an indented "key:" inside the patch block
// that starts at patch_index. Returns -1 when absent.
int FindInPatch(const std::vector<std::wstring>& lines,
                int patch_index,
                const std::wstring& key) {
  for (size_t i = patch_index + 1; i < lines.size(); ++i) {
    if (lines[i].empty())
      continue;
    if (!IsIndented(lines[i]))
      break;
    std::wstring t = Trim(lines[i]);
    if (t.rfind(key + L":", 0) == 0)
      return (int)i;
  }
  return -1;
}

// Find the top-level "patch:" line index, or -1.
int FindPatchBlock(const std::vector<std::wstring>& lines) {
  for (size_t i = 0; i < lines.size(); ++i) {
    std::wstring t = Trim(lines[i]);
    if (t == L"patch:" || t.rfind(L"patch:", 0) == 0)
      return (int)i;
  }
  return -1;
}

std::wstring GetSkinFromFile(const fs::path& p, bool top_level) {
  std::vector<std::wstring> lines = ReadLines(p);
  if (top_level) {
    // weasel.yaml: find top-level "style:" then indented "skin:"
    for (size_t i = 0; i < lines.size(); ++i) {
      if (!IsIndented(lines[i]) && Trim(lines[i]) == L"style:") {
        for (size_t j = i + 1; j < lines.size(); ++j) {
          if (!IsIndented(lines[j]))
            break;
          std::wstring v = ValueOf(lines[j], L"skin");
          if (!v.empty())
            return v;
        }
        break;
      }
    }
    return L"";
  }
  int patch = FindPatchBlock(lines);
  if (patch < 0)
    return L"";
  int idx = FindInPatch(lines, patch, L"style/skin");
  if (idx < 0)
    return L"";
  return ValueOf(lines[idx], L"style/skin");
}

}  // namespace

std::vector<std::wstring> ListSkinFiles() {
  std::vector<std::wstring> files;
  std::wstring pattern = UserDir().wstring() + L"\\*.ssf";
  WIN32_FIND_DATAW fd = {0};
  HANDLE h = FindFirstFileW(pattern.c_str(), &fd);
  if (h == INVALID_HANDLE_VALUE)
    return files;
  do {
    if (!(fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY))
      files.push_back(fd.cFileName);
  } while (FindNextFileW(h, &fd));
  FindClose(h);
  std::sort(files.begin(), files.end());
  return files;
}

std::wstring GetActiveSkin() {
  fs::path custom = UserDir() / L"weasel.custom.yaml";
  if (fs::exists(custom)) {
    std::wstring v = GetSkinFromFile(custom, false);
    if (!v.empty())
      return v;
  }
  fs::path weasel = UserDir() / L"weasel.yaml";
  if (fs::exists(weasel))
    return GetSkinFromFile(weasel, true);
  return L"";
}

bool SetActiveSkin(const std::wstring& file_name) {
  fs::path p = UserDir() / L"weasel.custom.yaml";
  std::vector<std::wstring> lines = ReadLines(p);
  std::wstring entry = L"  style/skin: \"" + file_name + L"\"";
  int patch = FindPatchBlock(lines);
  if (patch < 0) {
    if (!lines.empty() && !Trim(lines.back()).empty())
      lines.push_back(L"");
    lines.push_back(L"patch:");
    lines.push_back(entry);
  } else {
    int idx = FindInPatch(lines, patch, L"style/skin");
    if (idx >= 0) {
      lines[idx] = entry;
    } else {
      // insert at the end of the patch block
      size_t insert_at = lines.size();
      for (size_t i = patch + 1; i < lines.size(); ++i) {
        if (!lines[i].empty() && !IsIndented(lines[i])) {
          insert_at = i;
          break;
        }
      }
      lines.insert(lines.begin() + insert_at, entry);
    }
  }
  return WriteLines(p, lines);
}

bool ClearActiveSkin() {
  fs::path p = UserDir() / L"weasel.custom.yaml";
  std::vector<std::wstring> lines = ReadLines(p);
  bool changed = false;
  std::vector<std::wstring> out;
  for (size_t i = 0; i < lines.size(); ++i) {
    if (IsIndented(lines[i]) &&
        Trim(lines[i]).rfind(L"style/skin:", 0) == 0) {
      changed = true;
      continue;
    }
    out.push_back(lines[i]);
  }
  if (!changed)
    return true;
  return WriteLines(p, out);
}

}  // namespace skin_settings
