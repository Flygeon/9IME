#pragma once
//
// SkinSettings.h - Sogou (.ssf) skin management for the 9IME deployer
//
// Skins live in the Rime user directory (%AppData%\Rime\*.ssf) and are
// activated through weasel.custom.yaml:
//
//   patch:
//     style/skin: "my-skin.ssf"
//

#include <string>
#include <vector>

namespace skin_settings {

// List *.ssf file names (base names) found in the Rime user directory.
std::vector<std::wstring> ListSkinFiles();

// The currently configured skin base name, read from weasel.custom.yaml
// (patch) first, then weasel.yaml (top-level style). Empty when none.
std::wstring GetActiveSkin();

// Set style/skin in weasel.custom.yaml. Returns false on I/O failure.
bool SetActiveSkin(const std::wstring& file_name);

// Remove style/skin from weasel.custom.yaml. Returns false on I/O failure.
bool ClearActiveSkin();

}  // namespace skin_settings
