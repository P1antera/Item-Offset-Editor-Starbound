# Starbound Item Pose Editor

一个用于在游戏外预览 Starbound ActiveItem 持物姿势并快速调整 Offset 的 Windows 小工具。它读取游戏与 Mod 的资源，不会修改 `assets.pak`；当前参数调整仅用于实时预览，不会写回 `.activeitem` 文件。

我使用 Codex 编写了这个小工具，以减少反复进入游戏测试物品 Offset 的时间。项目仍处于早期阶段，后续可能会根据我的需求继续增加功能和渲染兼容性。

## 使用

项目根目录附带 `StarboundItemPoseEditor.exe`。将整个文件夹放在 Starbound 根目录下可自动发现资源；也可以将它移到别处，并在界面中选择 Starbound 根目录。双击 EXE 即可使用，不需要安装 Rust、Node.js 或重新构建。

目前支持固定模板与随机模板枪械的静态预览、手臂瞄准、左右朝向、基础种族模板，以及带注释或字符串原始换行的 Mod ActiveItem 文件。

## 开发构建

需要 Rust（MSVC 工具链）、Node.js 20+，以及 Visual Studio 2022 Build Tools 的 **Desktop development with C++** 工作负载。在此目录运行：

```powershell
npm install
npm run build
```
