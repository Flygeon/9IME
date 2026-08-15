# 9IME 輸入法

9IME 是一款基於 [小狼毫 / Weasel](https://github.com/rime/weasel)（Rime 輸入法引擎 Windows 發行版）二次開發的開源輸入法，
在保留 Rime 全部功能的基礎上，新增對 **搜狗輸入法皮膚（.ssf）** 的兼容支持，可自由更換皮膚。

[![Build status](https://github.com/Flygeon/9IME/actions/workflows/ci.yml/badge.svg)](https://github.com/Flygeon/9IME/actions/workflows/ci.yml)

授權條款：GPLv3（繼承自小狼毫）

## 特性

* 完整繼承 Rime / 小狼毫能力：朙月拼音、注音、倉頡、五筆等輸入方案，詞庫、用戶詞典、同步等
* **搜狗皮膚兼容**：直接使用 `.ssf` 皮膚文件（支持普通 zip 與搜狗加密容器兩種格式），
  自動解析 `skin.ini`，渲染候選窗背景（九宮格拉伸）、拼音區、候選高亮、狀態欄圖標
* 不改變 Rime 用戶目錄（`%AppData%\Rime`）與配置體系，遷移零成本

## 使用皮膚

1. 下載任意搜狗輸入法皮膚（`.ssf` 文件），放入 Rime 用戶目錄（`%AppData%\Rime`）
2. 在 `weasel.yaml`（或 `default.custom.yaml`）中指定皮膚文件：

```yaml
patch:
  style/skin: "皮膚文件名.ssf"   # 相對路徑以用戶目錄為基準
```

3. 重新部署（開始菜單 » 9IME » 重新部署），生效即時

去掉 `style/skin` 配置並重新部署，即可恢復默認配色界面。

> 示例皮膚見 [skins/sample.ssf](skins/sample.ssf)（解包內容見 [skins/sample/](skins/sample/)），
> 可直接複製到用戶目錄體驗。

### 皮膚支持範圍（v1）

* 候選窗背景圖：橫排（`Scheme_H1`）與豎排（`Scheme_V1`），九宮格拉伸（`layout_horizontal` / `layout_vertical`）
* 拼音區 / 候選區邊距（`pinyin_marge` / `zhongwen_marge`）、分隔線（`separator`）
* 高亮背景圖（`Scheme_H2` / `Scheme_V2` 的 `pinyin_pic` / `zhongwen_pic`）
* 字體與顏色（`[Display]`：`font_size`、`font_ch`、`pinyin_color`、`zhongwen_color`、`zhongwen_first_color`）
* 狀態欄：`bar.png` 背景 + 中英文（`cn_en`）、全半角（`quan_ban`）等按鈕圖標按 `_pos` 位置繪製
* 皮膚坐標均按 96 DPI 設計，運行時按顯示器 DPI 自動縮放

暫不支持：狀態欄按鈕點擊交互、皮膚內置菜單、動畫效果。

## 構建

本項目使用 GitHub Actions 自動構建（MSBuild 與 xmake 雙通道），產出安裝包上傳至 Actions Artifacts：

```yaml
# .github/workflows/ci.yml
# 構建環境：windows-2022 + Boost 1.84 + librime 預編譯庫
```

本地構建（需要 Visual Studio 2022 + Boost）：

```bat
copy env.vs2022.bat env.bat
rem 編輯 env.bat 設置 BOOST_ROOT
build.bat arm64 installer
```

## 安裝輸入法

適用於 Windows 8.1 ~ Windows 11。初次安裝時，安裝程序將顯示「安裝選項」對話框，
可選擇註冊到簡體中文 / 繁體中文 / 香港 / 澳門 / 新加坡鍵盤佈局。

## 定制輸入法

用戶詞庫、配置文件位於 `%AppData%\Rime`，可通過托盤菜單「用戶文件夾」打開。
修改後須「重新部署」方可生效。Rime 定制方法請參考 [《定製指南》](https://github.com/rime/home/wiki/CustomizationGuide)。

## 與小狼毫的關係

9IME 是小狼毫（Weasel）的衍生發行版，主要差異：

* 新增搜狗 `.ssf` 皮膚渲染引擎（`WeaselUI/SogouSkin.*`，內置 miniz 解壓與 BCrypt AES 解密）
* 品牌名稱改為 9IME，TSF 文本服務使用新的 CLSID/GUID，與小狼毫互不衝突，可並存安裝
* 註冊表鍵改為 `Software\Rime\9IME`；關閉了 WinSparkle 自動更新（改用 GitHub Releases 發佈）
* 內部可執行文件名（WeaselServer.exe 等）與數據目錄沿用上游命名，以降低維護成本

## 致謝

* [Rime / 小狼毫](https://github.com/rime/weasel) 及其所有[代碼貢獻者](https://github.com/rime/weasel/graphs/contributors)
* [ssfconv](https://github.com/fkxxyz/ssfconv)（搜狗皮膚格式分析參考）
* [miniz](https://github.com/richgel999/miniz)（公共領域 zip/zlib 庫）
* Boost、librime、OpenCC、plum、WinSparkle 等上游開源項目

## 問題與反饋

發現 bug 或有建議，請到 <https://github.com/Flygeon/9IME/issues> 反饋。

謝謝！
