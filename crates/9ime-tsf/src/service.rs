//! The TSF text service: activation, key events, edit sessions.
//!
//! M2 scope: activate the service, catch keys, and commit text through the
//! focused context via a placeholder engine. The librime engine arrives in
//! M3; the plumbing here (sinks, edit sessions) is final.

use std::sync::{Arc, Mutex};
use windows::core::{implement, Interface, Ref, Result, GUID, BOOL, HRESULT};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfDocumentMgr, ITfEditSession, ITfEditSession_Impl,
    ITfInsertAtSelection, ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr,
    ITfSource, ITfTextInputProcessor, ITfTextInputProcessorEx,
    ITfTextInputProcessorEx_Impl, ITfTextInputProcessor_Impl, ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
    INSERT_TEXT_AT_SELECTION_FLAGS, TF_ES_ASYNCDONTCARE, TF_ES_READWRITE,
};

use crate::engine;

fn e_fail() -> windows::core::Error {
    HRESULT::from_win32(0x80004005).into()
}

/// State shared between the service and its sinks.
pub struct Shared {
    pub client_id: u32,
    pub thread_mgr: Option<ITfThreadMgr>,
    pub key_advised: bool,
    pub focus: Option<ITfDocumentMgr>,
}

pub struct ServiceState {
    pub shared: Option<Arc<Mutex<Shared>>>,
    pub event_cookie: u32,
}

/// The text service object handed to TSF via the class factory.
#[implement(ITfTextInputProcessor, ITfTextInputProcessorEx)]
pub struct TextService {
    pub state: Mutex<ServiceState>,
}

impl TextService {
    pub fn new() -> Self {
        TextService {
            state: Mutex::new(ServiceState { shared: None, event_cookie: 0 }),
        }
    }

    fn activate_inner(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        if st.shared.is_some() {
            return Ok(()); // already active
        }
        let tm: ITfThreadMgr = ptim.cloned().ok_or_else(e_fail)?;
        let shared = Arc::new(Mutex::new(Shared {
            client_id: tid,
            thread_mgr: Some(tm.clone()),
            key_advised: false,
            focus: None,
        }));

        // 1. thread manager event sink (focus tracking).
        let sink: ITfThreadMgrEventSink =
            ThreadMgrEventSink { shared: shared.clone() }.into();
        let source: ITfSource = tm.cast()?;
        let cookie = unsafe {
            source.AdviseSink(&<ITfThreadMgrEventSink as Interface>::IID, &sink)
        }?;

        // 2. key event sink (foreground keys).
        let keysink: ITfKeyEventSink = KeyEventSink {
            shared: shared.clone(),
            test_pending: Mutex::new(false),
        }.into();
        let km: ITfKeystrokeMgr = tm.cast()?;
        unsafe { km.AdviseKeyEventSink(tid, &keysink, true.into()) }?;
        shared.lock().unwrap().key_advised = true;

        st.shared = Some(shared);
        st.event_cookie = cookie;
        Ok(())
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        self.activate_inner(ptim, tid)
    }

    fn Deactivate(&self) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        let Some(shared) = st.shared.take() else {
            return Ok(());
        };
        let (tid, tm) = {
            let s = shared.lock().unwrap();
            (s.client_id, s.thread_mgr.clone())
        };
        if let Some(tm) = tm {
            if let Ok(km) = tm.cast::<ITfKeystrokeMgr>() {
                let _ = unsafe { km.UnadviseKeyEventSink(tid) };
            }
            if st.event_cookie != 0 {
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(st.event_cookie) };
                }
                st.event_cookie = 0;
            }
        }
        Ok(())
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(
        &self,
        ptim: Ref<ITfThreadMgr>,
        tid: u32,
        _dwflags: u32,
    ) -> Result<()> {
        self.activate_inner(ptim, tid)
    }
}

/// Tracks document manager focus changes (needed by M3 composition).
#[implement(ITfThreadMgrEventSink)]
pub struct ThreadMgrEventSink {
    pub shared: Arc<Mutex<Shared>>,
}

impl ITfThreadMgrEventSink_Impl for ThreadMgrEventSink_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnUninitDocumentMgr(&self, _pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnSetFocus(
        &self,
        pdimfocus: Ref<ITfDocumentMgr>,
        _pdimprevfocus: Ref<ITfDocumentMgr>,
    ) -> Result<()> {
        if let Ok(mut s) = self.shared.lock() {
            s.focus = pdimfocus.cloned();
        }
        Ok(())
    }

    fn OnPushContext(&self, _pic: Ref<ITfContext>) -> Result<()> {
        Ok(())
    }

    fn OnPopContext(&self, _pic: Ref<ITfContext>) -> Result<()> {
        Ok(())
    }
}

/// Receives key events from TSF and forwards them to the engine.
#[implement(ITfKeyEventSink)]
pub struct KeyEventSink {
    pub shared: Arc<Mutex<Shared>>,
    pub test_pending: Mutex<bool>,
}

impl KeyEventSink {
    fn process_key(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<bool> {
        let _ = lparam;
        let keycode = wparam.0 as u32;
        let mask = engine::current_mask();
        let ke = engine::KeyEvent { keycode, mask };
        match engine::process(&ke) {
            engine::EngineOutput::Passthrough => Ok(false),
            engine::EngineOutput::Handled { commit } => {
                if let Some(text) = commit {
                    self.commit_text(pic, &text)?;
                }
                Ok(true)
            }
        }
    }

    /// Queue an edit session that inserts `text` at the selection.
    fn commit_text(&self, pic: Ref<ITfContext>, text: &str) -> Result<()> {
        let tid = self.shared.lock().unwrap().client_id;
        let ctx: ITfContext = pic.cloned().ok_or_else(e_fail)?;
        let edit: ITfEditSession = EditSession {
            context: ctx.clone(),
            action: Mutex::new(EditAction::Insert(text.to_string())),
        }.into();
        let _ = unsafe {
            ctx.RequestEditSession(tid, &edit, TF_ES_ASYNCDONTCARE | TF_ES_READWRITE)
        }?;
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for KeyEventSink_Impl {
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        pic: Ref<ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        // Some apps send multiple OnTestKeyDown per key event.
        let mut p = self.test_pending.lock().unwrap();
        if *p {
            return Ok(true.into());
        }
        let eaten = self.process_key(pic, wparam, lparam)?;
        *p = eaten;
        Ok(eaten.into())
    }

    fn OnKeyDown(
        &self,
        pic: Ref<ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        let mut p = self.test_pending.lock().unwrap();
        if *p {
            *p = false;
            return Ok(true.into());
        }
        let eaten = self.process_key(pic, wparam, lparam)?;
        Ok(eaten.into())
    }

    fn OnTestKeyUp(
        &self,
        _pic: Ref<ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        // A key-up cancels any pending test-key-down state.
        *self.test_pending.lock().unwrap() = false;
        Ok(false.into())
    }

    fn OnKeyUp(
        &self,
        _pic: Ref<ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        *self.test_pending.lock().unwrap() = false;
        Ok(false.into())
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(false.into())
    }
}

/// One-shot edit session carrying an action to perform inside the context.
#[implement(ITfEditSession)]
pub struct EditSession {
    pub context: ITfContext,
    pub action: Mutex<EditAction>,
}

#[derive(Clone)]
pub enum EditAction {
    None,
    Insert(String),
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let action = self.action.lock().unwrap().clone();
        match action {
            EditAction::None => Ok(()),
            EditAction::Insert(text) => {
                let insert: ITfInsertAtSelection = self.context.cast()?;
                let wide: Vec<u16> = text.encode_utf16().collect();
                unsafe {
                    insert.InsertTextAtSelection(
                        ec,
                        INSERT_TEXT_AT_SELECTION_FLAGS(0),
                        &wide,
                    )
                }?;
                Ok(())
            }
        }
    }
}
