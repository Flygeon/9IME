# 皮肤示例 (Sample Skins)

本目录包含一个搜狗输入法皮肤示例（.ssf 格式），用于演示 9IME 的皮肤兼容功能。

## 使用方式

1. 将 [sample.ssf](sample.ssf) 复制到 Rime 用户目录（%AppData%\Rime）
2. 在 weasel.yaml 中添加：

```yaml
patch:
  style/skin: "sample.ssf"
```

3. 重新部署即可生效。

## 目录说明

| 文件 | 说明 |
| --- | --- |
| sample.ssf | 皮肤安装包（zip 格式，52 个文件） |
| sample/ | 上述皮肤的原始解包内容，供参考 |

sample/skin.ini 关键配置：

- [Scheme_H1]：横排候选窗方案（skin1.png 背景，九宫格拉伸边距 layout_horizontal=1,103,305）
- [Scheme_V1]：竖排候选窗方案（skin2.png 背景）
- [StatusBar]：状态栏（bar.png 背景 + 各状态按钮图标及坐标）
- [Display]：字体（荆南麦圆体 24px）与颜色（注意搜狗颜色为 BGR 序）

> 该皮肤作者为「匿名」，来源于搜狗皮肤站公开示例，仅用于兼容性测试。
