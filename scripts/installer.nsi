; 9IME v2 installer
Unicode true
Name "9IME"
!cd "..\target\release"
OutFile "9IME-Setup-0.1.0.exe"
InstallDir "$PROGRAMFILES64\9IME"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

!include "MUI2.nsh"
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "9IME" SEC_MAIN
  SetOutPath "$INSTDIR"
  File "/oname=nineime_tsf.dll" "nineime_tsf.dll"
  File "nineime-server.exe"
  File "nineime-deployer.exe"
  File "nineime-console.exe"
  File "rime.dll"
  File /r "data\*.*"
  ; register the TSF text service (CLSID + TIP)
  ExecWait 'regsvr32 /s "$INSTDIR\nineime_tsf.dll"'
  WriteRegStr HKLM "SOFTWARE\9IME" "InstallDir" "$INSTDIR"
  CreateDirectory "$APPDATA\9IME\skins"
  CreateDirectory "$SMPROGRAMS\9IME"
  CreateShortcut "$SMPROGRAMS\9IME\9IME 设置.lnk" "$INSTDIR\nineime-deployer.exe"
  CreateShortcut "$DESKTOP\9IME 设置.lnk" "$INSTDIR\nineime-deployer.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  ExecWait 'regsvr32 /u /s "$INSTDIR\nineime_tsf.dll"'
  Delete "$INSTDIR\nineime_tsf.dll"
  Delete "$INSTDIR\nineime-server.exe"
  Delete "$INSTDIR\nineime-deployer.exe"
  Delete "$INSTDIR\nineime-console.exe"
  Delete "$INSTDIR\rime.dll"
  RMDir /r "$INSTDIR\data"
  RMDir /r "$INSTDIR"
  Delete "$SMPROGRAMS\9IME\9IME 设置.lnk"
  RMDir "$SMPROGRAMS\9IME"
  Delete "$DESKTOP\9IME 设置.lnk"
  DeleteRegKey HKLM "SOFTWARE\9IME"
SectionEnd
