# nlbn 自用优化版

这是我基于原项目改出的自用版本，重点针对 **macOS + KiCad** 元件库导出流程优化，不是通用发行版。

- 本仓库地址：https://github.com/Alddp/nlbn
- 原项目地址：https://github.com/linkyourbin/nlbn
- 许可证：CC BY-NC 4.0，继承自原项目

## 和原项目的区别

- 面向 macOS 和 KiCad 使用场景调整，Windows 未经测试。
- 移除了旧的 KiCad v5 兼容路径，不再以 KiCad v5 为兼容目标。
- 符号、封装、3D 模型分别拥有独立覆盖参数。
- 默认保留已有库资产，只有显式指定覆盖时才替换。
- 通过输出目录中的 `.checkpoint` 文件支持批处理续跑。
- 增加批量导出的进度报告和运行汇总。
- 支持 `--project-relative`，为 3D 模型生成 `${KIPRJMOD}` 引用。
- 支持 `--symbol-fill-color`，可覆盖符号填充色。
- 重构导入/导出流水线，让转换选项边界更清晰。
- 增加覆盖行为和 3D 模型引用生成测试。

这个版本是 SeEx 自用优化版预期使用的 `nlbn`：

https://github.com/Alddp/seex

## 平台和兼容性说明

- 主要目标环境：macOS + KiCad。
- Windows 未经测试。
- KiCad v5 兼容已经移除。
- 这个仓库按个人 KiCad 元件库维护流程优化，功能取舍不以通用发行版为目标。

## 常用新增参数

```text
--overwrite-symbol      只覆盖符号输出
--overwrite-footprint   只覆盖封装输出
--overwrite-3d          只覆盖 3D 模型输出
--project-relative      使用 ${KIPRJMOD} 路径引用 3D 模型
--symbol-fill-color     覆盖符号填充色
```

## License

This project is licensed under CC BY-NC 4.0.
