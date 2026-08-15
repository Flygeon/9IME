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

## M3 (next): engine server process + named-pipe IPC + Direct2D candidate
window; TSF DLL forwards keys to the server which owns librime.

## M4: .ssf skin parsing + rendering + egui deployer (import/select/preview).
## M5: installer + skin hot-reload.
## M6: CI workflow + squash-rewrite of the Flygeon/9IME repository.
