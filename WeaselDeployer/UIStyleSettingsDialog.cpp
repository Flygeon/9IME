#include "stdafx.h"
#include "UIStyleSettingsDialog.h"
#include "UIStyleSettings.h"
#include "Configurator.h"
#include "SkinSettings.h"
#include <WeaselUtility.h>
#include <filesystem>

UIStyleSettingsDialog::UIStyleSettingsDialog(UIStyleSettings* settings)
    : settings_(settings),
      loaded_(false),
      skin_changed_(false),
      skin_pending_(0) {}

UIStyleSettingsDialog::~UIStyleSettingsDialog() {
  image_.Destroy();
}

void UIStyleSettingsDialog::Populate() {
  if (!settings_)
    return;
  std::string active(settings_->GetActiveColorScheme());
  int active_index = -1;
  settings_->GetPresetColorSchemes(&preset_);
  for (size_t i = 0; i < preset_.size(); ++i) {
    std::wstring txt = u8tow(preset_[i].name);
    color_schemes_.AddString(txt.c_str());
    if (preset_[i].color_scheme_id == active) {
      active_index = i;
    }
  }
  if (active_index >= 0) {
    color_schemes_.SetCurSel(active_index);
    Preview(active_index);
  }
  loaded_ = true;
}

LRESULT UIStyleSettingsDialog::OnInitDialog(UINT, WPARAM, LPARAM, BOOL&) {
  color_schemes_.Attach(GetDlgItem(IDC_COLOR_SCHEME));
  preview_.Attach(GetDlgItem(IDC_PREVIEW));
  select_font_.Attach(GetDlgItem(IDC_SELECT_FONT));
  select_font_.EnableWindow(FALSE);

  // 9IME: Sogou skin (.ssf) management
  skin_list_.Attach(GetDlgItem(IDC_SKIN_COMBO));
  import_skin_.Attach(GetDlgItem(IDC_IMPORT_SKIN));
  remove_skin_.Attach(GetDlgItem(IDC_REMOVE_SKIN));

  Populate();
  PopulateSkins();

  CenterWindow();
  BringWindowToTop();
  return TRUE;
}

// 9IME: fill the skin combo box with *.ssf files from the user directory
void UIStyleSettingsDialog::PopulateSkins() {
  skin_files_ = skin_settings::ListSkinFiles();
  skin_list_.ResetContent();
  skin_list_.AddString(L"（不使用皮肤）");
  for (const auto& f : skin_files_)
    skin_list_.AddString(f.c_str());

  std::wstring active = skin_settings::GetActiveSkin();
  int index = 0;
  for (size_t i = 0; i < skin_files_.size(); ++i) {
    if (_wcsicmp(skin_files_[i].c_str(), active.c_str()) == 0) {
      index = (int)i + 1;
      break;
    }
  }
  skin_list_.SetCurSel(index);
  skin_pending_ = index;
  skin_changed_ = false;
}

// 9IME: select an entry in the skin combo by file name
void UIStyleSettingsDialog::SelectSkin(const std::wstring& file_name) {
  int index = 0;
  for (size_t i = 0; i < skin_files_.size(); ++i) {
    if (_wcsicmp(skin_files_[i].c_str(), file_name.c_str()) == 0) {
      index = (int)i + 1;
      break;
    }
  }
  skin_list_.SetCurSel(index);
  skin_pending_ = index;
  skin_changed_ = true;
}

// 9IME: write the pending skin selection into weasel.custom.yaml
bool UIStyleSettingsDialog::ApplySkinSelection() {
  if (!skin_changed_)
    return true;
  if (skin_pending_ <= 0 || skin_pending_ > (int)skin_files_.size())
    return skin_settings::ClearActiveSkin();
  return skin_settings::SetActiveSkin(skin_files_[skin_pending_ - 1]);
}

LRESULT UIStyleSettingsDialog::OnClose(UINT, WPARAM, LPARAM, BOOL&) {
  EndDialog(IDCANCEL);
  return 0;
}

LRESULT UIStyleSettingsDialog::OnOK(WORD, WORD code, HWND, BOOL&) {
  int sel = skin_list_.GetCurSel();
  if (sel >= 0)
    skin_pending_ = sel;
  if (!ApplySkinSelection()) {
    MessageBoxW(m_hWnd, L"写入 weasel.custom.yaml 失败，皮肤未应用。",
                L"9IME", MB_OK | MB_ICONERROR);
  }
  EndDialog(code);
  return 0;
}

LRESULT UIStyleSettingsDialog::OnColorSchemeSelChange(WORD, WORD, HWND, BOOL&) {
  int index = color_schemes_.GetCurSel();
  if (index >= 0 && index < (int)preset_.size()) {
    settings_->SelectColorScheme(preset_[index].color_scheme_id);
    Preview(index);
  }
  return 0;
}

LRESULT UIStyleSettingsDialog::OnImportSkin(WORD, WORD, HWND, BOOL&) {
  CFileDialog dlg(TRUE, L"ssf", NULL,
                  OFN_FILEMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR,
                  L"搜狗输入法皮肤 (*.ssf)\0*.ssf\0所有文件 (*.*)\0*.*\0",
                  m_hWnd);
  if (dlg.DoModal() != IDOK)
    return 0;
  std::wstring src = dlg.m_szFileName;
  std::wstring name = std::filesystem::path(src).filename().wstring();
  std::filesystem::path dst =
      WeaselUserDataPath() / name;
  if (!CopyFileW(src.c_str(), dst.c_str(), FALSE)) {
    DWORD err = GetLastError();
    if (err != ERROR_ALREADY_EXISTS || !CopyFileW(src.c_str(), dst.c_str(), TRUE)) {
      MessageBoxW(m_hWnd, L"导入皮肤失败，请检查文件权限。", L"9IME",
                  MB_OK | MB_ICONERROR);
      return 0;
    }
  }
  PopulateSkins();
  SelectSkin(name);
  return 0;
}

LRESULT UIStyleSettingsDialog::OnRemoveSkin(WORD, WORD, HWND, BOOL&) {
  skin_list_.SetCurSel(0);
  skin_pending_ = 0;
  skin_changed_ = true;
  return 0;
}

LRESULT UIStyleSettingsDialog::OnSkinSelChange(WORD, WORD, HWND, BOOL&) {
  int sel = skin_list_.GetCurSel();
  if (sel >= 0) {
    skin_pending_ = sel;
    skin_changed_ = true;
  }
  return 0;
}

void UIStyleSettingsDialog::Preview(int index) {
  if (index < 0 || index >= (int)preset_.size())
    return;
  const std::string file_path(
      settings_->GetColorSchemePreview(preset_[index].color_scheme_id));
  if (file_path.empty())
    return;
  image_.Destroy();
  // it is from ansi coding, not utf8
  image_.Load(acptow(file_path).c_str());
  if (!image_.IsNull()) {
    preview_.SetBitmap(image_);
  }
}
