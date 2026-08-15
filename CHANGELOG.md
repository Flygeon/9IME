<a name="9ime-0.1.0"></a>
## 9IME 0.1.0 (2026-08-15)

### 主要更新

* **9IME 品牌化**：基於小狼毫 0.17.4 的衍生發行版；TSF 名稱、安裝包、托盤、對話框統一為 9IME
* **新增搜狗輸入法皮膚（.ssf）兼容支持**：
  * 支持普通 zip 與搜狗加密容器兩種 `.ssf` 格式（AES-256-CBC 解密 + zlib 解壓 + 自定義容器解析）
  * 解析 `skin.ini`（UTF-16 / UTF-8 / GBK 自動識別），支持 `[General]`、`[Display]`、`[Scheme_H1/H2/V1/V2]`、`[StatusBar]`
  * 候選窗背景九宮格拉伸渲染、拼音區/候選區定位、分隔線、高亮背景圖、狀態欄（bar.png + 狀態圖標）
  * 皮膚坐標按 96 DPI 設計並隨顯示器 DPI 自動縮放
  * 配置方式：`weasel.yaml` 中設置 `style/skin: 皮膚文件.ssf`（置於 Rime 用戶目錄）
* **TSF 文本服務使用全新 CLSID/GUID**，與小狼毫可並存安裝
* **註冊表鍵遷移至** `Software\Rime\9IME`；關閉 WinSparkle 自動更新，改為打開 GitHub Releases 頁面
* **CI 構建**：GitHub Actions（MSBuild + xmake 雙通道，windows-2022），產出 `9ime-*-installer.exe`
* 新增內置依賴：miniz（公共領域 zip/zlib）、BCrypt（AES 解密）

### 已知限制

* 皮膚狀態欄按鈕暫不支持點擊交互
* 皮膚內置菜單、動畫效果暫不支持
* 內部可執行文件名（WeaselServer.exe 等）沿用上游命名

<a name="0.17.4"></a>
## [0.17.4](https://github.com/rime/weasel/compare/0.17.3...0.17.4)(2025-06-04)