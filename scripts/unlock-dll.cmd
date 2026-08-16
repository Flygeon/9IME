@echo off
rem 解锁被系统占用的 nineime_tsf.dll，使新构建能够写入。
rem 用法: 右键 -> 以管理员身份运行。
setlocal
set "DIR=%ProgramFiles%\9IME"
if not exist "%DIR%\nineime_tsf.dll" (
  echo [跳过] 未找到 %DIR%\nineime_tsf.dll
  goto :done
)

echo [1/3] 注销 TSF 文本服务...
regsvr32 /u /s "%DIR%\nineime_tsf.dll"

echo [2/3] 停止输入服务...
taskkill /f /im nineime-server.exe >nul 2>&1

echo [3/3] 删除被占用的 DLL...
del /f /q "%DIR%\nineime_tsf.dll" 2>nul

if exist "%DIR%\nineime_tsf.dll" (
  echo.
  echo [警告] DLL 仍被其他应用占用 ^(chrome/explorer 等^)。
  echo        请关闭这些应用或注销/重启后重试。
) else (
  echo.
  echo [完成] DLL 已释放，现在可以安装新构建。
)

:done
echo.
pause
