# 9IME v2 (Rust) — milestone status

## M1 engine bindings (done)
- crates/9ime-librime: librime C ABI FFI (rime_api.h field order) + safe-ish
  wrapper; dynamic loading via libloading (no link-time dependency).
- crates/9ime-core: shared foundation (skin parsing arrives in M3/M4).
- crates/9ime-console: smoke-test binary (load, initialize, deploy, session,
  key sequence, commit/context/status dump).

## M2 TSF text service (done at code level)
- crates/9ime-tsf: COM server DLL (cdylib).
  - DllGetClassObject / DllCanUnloadNow / DllRegisterServer / DllUnregisterServer
  - ITfTextInputProcessor / ITfTextInputProcessorEx
  - ITfThreadMgrEventSink (focus tracking)
  - ITfKeyEventSink (test-down/down/up paths, dup-test suppression)
  - ITfEditSession one-shot commits via ITfInsertAtSelection
  - placeholder engine (M2): commits printable ASCII; librime arrives in M3

Local validation: `cargo +stable-x86_64-pc-windows-gnu check --workspace`
(the machine has no MSVC linker; real builds/tests run in GitHub Actions).

### Manual M2 test on a real machine (after CI build)
1. Copy nineime_tsf.dll somewhere; run as admin:
   `regsvr32 nineime_tsf.dll`
2. Add the TIP: Settings > Time & Language > Typing > Advanced keyboard
   settings, or classic control panel - the service registers as zh-CN
   profile "9IME".
3. Switch to 9IME in an editor; typing letters/digits should insert them
   (placebo engine), proving activate -> key sink -> edit session pipeline.
4. Remove: `regsvr32 /u nineime_tsf.dll`

## M3 engine server + candidate window (done at code level)
- crates/9ime-ipc: length-prefixed JSON protocol over \\.\pipe\9ime.
- crates/9ime-server: owns librime (one session, one thread), named-pipe
  server, GDI candidate window (caret-anchored, topmost).
- crates/9ime-tsf: IPC client replaces the M2 placeholder; launches the
  server next to the DLL on first key.

## M4 skin + deployer (done at code level)
- nineime-core: .ssf containers (zip + encrypted "Skin" AES-256-CBC zlib
  archive), skin.ini parsing (UTF-16LE/UTF-8/GBK), skin model (9-slice
  margins, pinyin/zhongwen marges, colors, fonts), 9ime.json config.
- server: loads the active skin from %APPDATA%\9IME\skins, renders
  background/highlight images 9-sliced via GDI, hot-reloads on config change.
- crates/9ime-deployer (egui): import/select/remove skins, deploy trigger.

## M5 (next): installer (NSIS) + packaging; CI artifacts.
## M6: CI workflow + squash-rewrite of the Flygeon/9IME repository.

## Post-M4 fix round (input actually works now)
- register: InprocServer32/icon path came from GetModuleFileNameW(NULL) =
  the host exe (regsvr32.exe) -> TSF could never load the service and all
  keys passed through as plain English. Now resolved from the DLL's own
  module handle (GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS).
- tsf: modifier mask now follows the X11 layout librime expects
  (Shift=1/Lock=2/Ctrl=4/Alt=8); full VK->X11 keysym map (BackSpace/Return/
  Escape/Delete/Home/End/F-keys/numpad/OEM punctuation); bare modifier and
  Alt+key presses pass through untouched.
- tsf: OnTestKeyDown/OnKeyDown tri-state - a key the engine saw during the
  test call is never fed twice (double letters / double commits).
- tsf client: WaitNamedPipe on busy pipe instead of killing the server.
- server: session is created lazily after the startup deploy (no more
  create_session during maintenance mode); multi-client named pipe
  (one rime thread fed via channel) so several apps can type at once;
  keys pass through while the first-run deploy is still building.
- window: layered per-pixel-alpha rendering, own 9-slice compositor for
  skin background/highlight (rounded corners/shadows render correctly),
  skin insets + separator + page indicator, work-area clamping,
  click-through, DPI aware.
- config: skin name resolves against the skins dir (case-insensitive,
  single-skin fallback) - survives stale/mojibake'd config values.
- deployer: redesigned UI (header/status bar/columns), skin preview with
  background image + colors + font info, import/delete with confirmation,
  open skins/log folders, restart server, deploy with elapsed time.
