use std::{
    collections::HashMap,
    ffi::c_void,
    io, mem, ptr,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use ::windows::{
    Win32::{
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IDataObject,
            },
            Ole::{
                CF_UNICODETEXT, OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard,
                OleUninitialize, SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound,
                SafeArrayGetUBound,
            },
        },
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation, IUIAutomationTextPattern2, UIA_TextPattern2Id,
        },
    },
    core::BOOL,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_SUCCESS, GetLastError, GlobalFree, LPARAM, LRESULT, POINT, RECT, SetLastError, WPARAM,
    },
    Graphics::Gdi::ClientToScreen,
    System::{
        DataExchange::{
            CloseClipboard, CountClipboardFormats, EmptyClipboard, GetClipboardSequenceNumber,
            OpenClipboard, SetClipboardData,
        },
        LibraryLoader::GetModuleHandleW,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyState, GetKeyboardLayout, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
            KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, ToUnicodeEx, VK_BACK, VK_CAPITAL,
            VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LCONTROL, VK_LEFT,
            VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
            VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_PERIOD, VK_PRIOR, VK_RCONTROL,
            VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
            VK_Z,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetMessageW,
            GetWindowRect, GetWindowThreadProcessId, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
            PostThreadMessageW, SetForegroundWindow, SetWindowsHookExW, UnhookWindowsHookEx,
            WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    },
};

use crate::core::{
    autocomplete::{AutocompleteEntry, AutocompleteIndex, Completion},
    autocorrect::{AutocorrectIndex, Correction},
    snippets::SnippetMatcher,
};

const EVENT_QUEUE_CAPACITY: usize = 512;
const INPUT_MARKER: usize = 0x5450_4C54;
const MAX_AUTOCOMPLETE_CANDIDATES: usize = 6;
const CLIPBOARD_RETRY_COUNT: usize = 10;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(10);
const CLIPBOARD_PASTE_DELAY: Duration = Duration::from_millis(150);

static HOOK_SENDER: OnceLock<SyncSender<KeyEvent>> = OnceLock::new();
static TAB_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static BOUNDARY_SUPPRESSED: AtomicU32 = AtomicU32::new(0);
static UNDO_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static QUICK_SEARCH_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static AUTOCOMPLETE_VISIBLE: AtomicBool = AtomicBool::new(false);
static AUTOCOMPLETE_NAV_SUPPRESSED: AtomicU32 = AtomicU32::new(0);

enum ClipboardSnapshot {
    Empty,
    Data(IDataObject),
}

struct OleGuard;

impl Drop for OleGuard {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

struct OpenClipboardGuard;

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

#[derive(Clone)]
pub struct SnippetController {
    inner: Arc<SnippetRuntime>,
}

struct SnippetRuntime {
    snippets: RwLock<HashMap<String, String>>,
    paused: AtomicBool,
}

impl SnippetController {
    pub fn new(snippets: Vec<(String, String)>) -> Self {
        let controller = Self {
            inner: Arc::new(SnippetRuntime {
                snippets: RwLock::new(HashMap::new()),
                paused: AtomicBool::new(false),
            }),
        };
        controller.replace_snippets(snippets);
        controller
    }

    pub fn replace_snippets(&self, snippets: Vec<(String, String)>) {
        let index = snippets
            .into_iter()
            .map(|(trigger, body)| (normalize_trigger(&trigger), body))
            .collect();

        if let Ok(mut current) = self.inner.snippets.write() {
            *current = index;
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    fn expansion_for(&self, trigger: &str) -> Option<String> {
        self.inner
            .snippets
            .read()
            .ok()?
            .get(&normalize_trigger(trigger))
            .cloned()
    }
}

#[derive(Debug, Clone)]
pub struct PositionedSuggestion {
    pub prefix: String,
    pub words: Vec<String>,
    pub selected_index: usize,
    pub anchor_x: i32,
    pub anchor_top: i32,
    pub anchor_bottom: i32,
}

type SuggestionPresenter = Arc<dyn Fn(Option<PositionedSuggestion>) + Send + Sync>;

#[derive(Clone)]
pub struct AutocompleteController {
    inner: Arc<AutocompleteRuntime>,
}

struct AutocompleteRuntime {
    index: RwLock<AutocompleteIndex>,
    session: RwLock<Option<AutocompleteSession>>,
    pending_replacement: Mutex<Option<LastReplacement>>,
    presenter: SuggestionPresenter,
}

#[derive(Clone)]
struct AutocompleteSession {
    prefix: String,
    completions: Vec<Completion>,
    selected_index: usize,
    target_window: isize,
    anchor: SuggestionAnchor,
}

#[derive(Clone, Copy)]
struct SuggestionAnchor {
    x: i32,
    top: i32,
    bottom: i32,
}

struct InputAnchorLocator {
    automation: Option<IUIAutomation>,
    com_initialized: bool,
}

impl InputAnchorLocator {
    fn new() -> Self {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let com_initialized = initialized.0 >= 0;
        let automation =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }.ok();

        Self {
            automation,
            com_initialized,
        }
    }

    fn focused_input_anchor(&self) -> Option<SuggestionAnchor> {
        let element = unsafe { self.automation.as_ref()?.GetFocusedElement() }.ok()?;

        if let Ok(pattern) =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) }
        {
            let mut active = BOOL::default();
            if let Ok(range) = unsafe { pattern.GetCaretRange(&mut active) }
                && let Ok(array) = unsafe { range.GetBoundingRectangles() }
                && let Some(anchor) = unsafe { anchor_from_bounding_rectangles(array) }
            {
                return Some(anchor);
            }
        }

        let rectangle = unsafe { element.CurrentBoundingRectangle() }.ok()?;
        anchor_from_rect(
            rectangle.left,
            rectangle.top,
            rectangle.right,
            rectangle.bottom,
        )
    }

    fn focused_input_is_password(&self) -> bool {
        let Some(automation) = self.automation.as_ref() else {
            return false;
        };
        let Ok(element) = (unsafe { automation.GetFocusedElement() }) else {
            return false;
        };

        unsafe { element.CurrentIsPassword() }.is_ok_and(|value| value.0 != 0)
    }
}

impl Drop for InputAnchorLocator {
    fn drop(&mut self) {
        if self.com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

unsafe fn anchor_from_bounding_rectangles(
    array: *mut ::windows::Win32::System::Com::SAFEARRAY,
) -> Option<SuggestionAnchor> {
    if array.is_null() {
        return None;
    }

    let result = (|| {
        let lower = unsafe { SafeArrayGetLBound(array, 1) }.ok()?;
        let upper = unsafe { SafeArrayGetUBound(array, 1) }.ok()?;
        if upper - lower + 1 < 4 {
            return None;
        }

        let mut values = [0.0f64; 4];
        for (offset, value) in values.iter_mut().enumerate() {
            let index = lower + offset as i32;
            unsafe { SafeArrayGetElement(array, &index, value as *mut f64 as *mut c_void) }.ok()?;
        }

        let left = values[0].round() as i32;
        let top = values[1].round() as i32;
        let width = values[2].round() as i32;
        let height = values[3].round() as i32;
        anchor_from_rect(left, top, left + width.max(1), top + height.max(18))
    })();

    let _ = unsafe { SafeArrayDestroy(array) };
    result
}

fn anchor_from_rect(left: i32, top: i32, right: i32, bottom: i32) -> Option<SuggestionAnchor> {
    if right <= left || bottom <= top {
        return None;
    }

    Some(SuggestionAnchor {
        x: left + 4,
        top,
        bottom,
    })
}

pub struct AcceptedAutocomplete {
    pub prefix: String,
    pub word: String,
    pub suffix: String,
    pub target_window: isize,
}

impl AutocompleteController {
    pub fn new<F>(words: Vec<AutocompleteEntry>, presenter: F) -> Self
    where
        F: Fn(Option<PositionedSuggestion>) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(AutocompleteRuntime {
                index: RwLock::new(AutocompleteIndex::new(words)),
                session: RwLock::new(None),
                pending_replacement: Mutex::new(None),
                presenter: Arc::new(presenter),
            }),
        }
    }

    pub fn replace_words(&self, words: Vec<AutocompleteEntry>) {
        if let Ok(mut index) = self.inner.index.write() {
            index.replace(words);
        }
        self.hide();
    }

    pub fn hide(&self) {
        AUTOCOMPLETE_VISIBLE.store(false, Ordering::Release);
        if let Ok(mut session) = self.inner.session.write() {
            *session = None;
        }
        (self.inner.presenter)(None);
    }

    pub fn select(&self, index: usize) {
        let suggestion = {
            let Ok(mut session) = self.inner.session.write() else {
                return;
            };
            let Some(session) = session.as_mut() else {
                return;
            };
            if index >= session.completions.len() {
                return;
            }
            session.selected_index = index;
            positioned_suggestion(session)
        };
        (self.inner.presenter)(Some(suggestion));
    }

    pub fn accept(&self, index: usize) -> Option<AcceptedAutocomplete> {
        let accepted = {
            let mut session = self.inner.session.write().ok()?;
            let session = session.take()?;
            let completion = session.completions.get(index)?.clone();
            AcceptedAutocomplete {
                prefix: session.prefix,
                word: completion.word,
                suffix: completion.suffix,
                target_window: session.target_window,
            }
        };
        AUTOCOMPLETE_VISIBLE.store(false, Ordering::Release);
        (self.inner.presenter)(None);
        Some(accepted)
    }

    pub fn record_external_accept(&self, accepted: &AcceptedAutocomplete) {
        if let Ok(mut pending) = self.inner.pending_replacement.lock() {
            *pending = Some(LastReplacement {
                original: accepted.prefix.clone(),
                replacement: accepted.word.clone(),
                boundary: None,
                window: accepted.target_window,
            });
        }
    }

    fn take_pending_replacement(&self) -> Option<LastReplacement> {
        self.inner.pending_replacement.lock().ok()?.take()
    }

    fn selected_completion(&self, prefix: &str) -> Option<Completion> {
        let session = self.inner.session.read().ok()?;
        let session = session.as_ref()?;
        if session.prefix != prefix {
            return None;
        }
        session.completions.get(session.selected_index).cloned()
    }

    fn move_selection(&self, delta: isize) {
        let suggestion = {
            let Ok(mut session) = self.inner.session.write() else {
                return;
            };
            let Some(session) = session.as_mut() else {
                return;
            };
            let count = session.completions.len() as isize;
            if count == 0 {
                return;
            }
            session.selected_index =
                (session.selected_index as isize + delta).rem_euclid(count) as usize;
            positioned_suggestion(session)
        };
        (self.inner.presenter)(Some(suggestion));
    }

    fn present_for(
        &self,
        prefix: &str,
        expected_window: isize,
        anchor_locator: &InputAnchorLocator,
    ) {
        let completions = self
            .inner
            .index
            .read()
            .ok()
            .map(|index| index.completions_for(prefix, MAX_AUTOCOMPLETE_CANDIDATES))
            .unwrap_or_default();
        if completions.is_empty() {
            self.hide();
            return;
        }

        if unsafe { GetForegroundWindow() } as isize != expected_window {
            self.hide();
            return;
        }

        let anchor = suggestion_screen_position(expected_window, anchor_locator);
        let session = AutocompleteSession {
            prefix: prefix.to_owned(),
            completions,
            selected_index: 0,
            target_window: expected_window,
            anchor,
        };
        let suggestion = positioned_suggestion(&session);
        if let Ok(mut current) = self.inner.session.write() {
            *current = Some(session);
        }
        AUTOCOMPLETE_VISIBLE.store(true, Ordering::Release);
        (self.inner.presenter)(Some(suggestion));
    }
}

fn positioned_suggestion(session: &AutocompleteSession) -> PositionedSuggestion {
    PositionedSuggestion {
        prefix: session.prefix.clone(),
        words: session
            .completions
            .iter()
            .map(|completion| completion.word.clone())
            .collect(),
        selected_index: session.selected_index,
        anchor_x: session.anchor.x,
        anchor_top: session.anchor.top,
        anchor_bottom: session.anchor.bottom,
    }
}

#[derive(Clone)]
pub struct AutocorrectController {
    index: Arc<RwLock<AutocorrectIndex>>,
}

impl AutocorrectController {
    pub fn new(words: Vec<String>) -> Self {
        Self {
            index: Arc::new(RwLock::new(AutocorrectIndex::new(words))),
        }
    }

    pub fn replace_words(&self, words: Vec<String>) {
        if let Ok(mut index) = self.index.write() {
            index.replace(words);
        }
    }

    fn correction_for(&self, word: &str) -> Option<Correction> {
        self.index.read().ok()?.correction_for(word)
    }
}

type QuickSearchPresenter = Arc<dyn Fn(isize, Option<(i32, i32)>) + Send + Sync>;

#[derive(Clone)]
pub struct QuickSearchController {
    presenter: QuickSearchPresenter,
}

impl QuickSearchController {
    pub fn new<F>(presenter: F) -> Self
    where
        F: Fn(isize, Option<(i32, i32)>) + Send + Sync + 'static,
    {
        Self {
            presenter: Arc::new(presenter),
        }
    }

    fn open(&self, target_window: isize, anchor: Option<(i32, i32)>) {
        (self.presenter)(target_window, anchor);
    }
}

pub struct InputMonitor {
    stop: Arc<AtomicBool>,
    hook_thread_id: u32,
    hook_thread: Option<JoinHandle<()>>,
    worker_thread: Option<JoinHandle<()>>,
}

impl InputMonitor {
    pub fn start(
        controller: SnippetController,
        autocomplete: AutocompleteController,
        autocorrect: AutocorrectController,
        quick_search: QuickSearchController,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        HOOK_SENDER.set(sender).map_err(|_| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "keyboard hook already started",
            )
        })?;

        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_thread = thread::Builder::new()
            .name("textpilot-snippet-worker".into())
            .spawn(move || {
                let mut worker =
                    SnippetWorker::new(controller, autocomplete, autocorrect, quick_search);
                while !worker_stop.load(Ordering::Acquire) {
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(event) => worker.handle(event),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;

        let (ready_sender, ready_receiver) = mpsc::channel();
        let hook_thread = thread::Builder::new()
            .name("textpilot-keyboard-hook".into())
            .spawn(move || run_hook_loop(ready_sender))?;

        let hook_thread_id = match ready_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(thread_id)) => thread_id,
            Ok(Err(error)) => {
                stop.store(true, Ordering::Release);
                let _ = worker_thread.join();
                let _ = hook_thread.join();
                return Err(error);
            }
            Err(_) => {
                stop.store(true, Ordering::Release);
                let _ = worker_thread.join();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "keyboard hook did not start",
                ));
            }
        };

        Ok(Self {
            stop,
            hook_thread_id,
            hook_thread: Some(hook_thread),
            worker_thread: Some(worker_thread),
        })
    }
}

impl Drop for InputMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        unsafe {
            PostThreadMessageW(self.hook_thread_id, WM_QUIT, 0, 0);
        }

        if let Some(thread) = self.hook_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.worker_thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone, Copy)]
enum KeyEventKind {
    Regular,
    Tab,
    Boundary,
    Undo,
    QuickSearch,
    AutocompletePrevious,
    AutocompleteNext,
    AutocompleteDismiss,
}

#[derive(Clone, Copy)]
struct KeyEvent {
    vk_code: u32,
    scan_code: u32,
    pressed: bool,
    kind: KeyEventKind,
    foreground_window: isize,
}

#[derive(Clone, Copy)]
struct KeyStroke {
    vk_code: u32,
    scan_code: u32,
}

impl From<KeyEvent> for KeyStroke {
    fn from(event: KeyEvent) -> Self {
        Self {
            vk_code: event.vk_code,
            scan_code: event.scan_code,
        }
    }
}

struct LastReplacement {
    original: String,
    replacement: String,
    boundary: Option<KeyStroke>,
    window: isize,
}

struct SnippetWorker {
    controller: SnippetController,
    autocomplete: AutocompleteController,
    autocorrect: AutocorrectController,
    quick_search: QuickSearchController,
    matcher: SnippetMatcher,
    modifiers: Modifiers,
    last_replacement: Option<LastReplacement>,
    anchor_locator: InputAnchorLocator,
    current_window: isize,
    current_window_is_own_process: bool,
    own_process_id: u32,
}

impl SnippetWorker {
    fn new(
        controller: SnippetController,
        autocomplete: AutocompleteController,
        autocorrect: AutocorrectController,
        quick_search: QuickSearchController,
    ) -> Self {
        Self {
            controller,
            autocomplete,
            autocorrect,
            quick_search,
            matcher: SnippetMatcher::default(),
            modifiers: Modifiers::default(),
            last_replacement: None,
            anchor_locator: InputAnchorLocator::new(),
            current_window: 0,
            current_window_is_own_process: false,
            own_process_id: unsafe { GetCurrentProcessId() },
        }
    }

    fn handle(&mut self, event: KeyEvent) {
        if let Some(replacement) = self.autocomplete.take_pending_replacement() {
            self.matcher.reset();
            self.last_replacement = Some(replacement);
        }

        if event.foreground_window != self.current_window {
            self.current_window = event.foreground_window;
            self.current_window_is_own_process =
                window_process_id(event.foreground_window) == self.own_process_id;
            self.matcher.reset();
            self.autocomplete.hide();
            self.last_replacement = None;
        }

        self.modifiers.update(event.vk_code, event.pressed);

        if !event.pressed {
            return;
        }

        if self.anchor_locator.focused_input_is_password() {
            self.matcher.reset();
            self.autocomplete.hide();
            self.last_replacement = None;
            replay_suppressed_event(event);
            return;
        }

        if matches!(event.kind, KeyEventKind::Undo) {
            self.handle_undo(event.foreground_window);
            return;
        }

        if matches!(event.kind, KeyEventKind::QuickSearch) {
            self.matcher.reset();
            self.autocomplete.hide();
            self.last_replacement = None;
            let anchor =
                quick_search_screen_position(event.foreground_window, &self.anchor_locator);
            self.quick_search.open(event.foreground_window, anchor);
            return;
        }

        if matches!(event.kind, KeyEventKind::AutocompletePrevious) {
            self.autocomplete.move_selection(-1);
            return;
        }

        if matches!(event.kind, KeyEventKind::AutocompleteNext) {
            self.autocomplete.move_selection(1);
            return;
        }

        if matches!(event.kind, KeyEventKind::AutocompleteDismiss) {
            self.autocomplete.hide();
            return;
        }

        if event.vk_code == u32::from(VK_TAB) {
            if matches!(event.kind, KeyEventKind::Tab) {
                self.handle_tab(event.foreground_window);
            } else {
                self.matcher.reset();
                self.autocomplete.hide();
            }
            return;
        }

        if matches!(event.kind, KeyEventKind::Boundary) {
            self.handle_boundary(event);
            return;
        }

        if self.current_window_is_own_process || self.controller.is_paused() {
            self.matcher.reset();
            self.autocomplete.hide();
            self.last_replacement = None;
            return;
        }

        if event.vk_code == u32::from(VK_BACK) {
            self.last_replacement = None;
            self.matcher.backspace();
            self.refresh_autocomplete();
            return;
        }

        if is_navigation_key(event.vk_code) || self.modifiers.command_modifier_active() {
            self.matcher.reset();
            self.autocomplete.hide();
            if !is_modifier_key(event.vk_code) {
                self.last_replacement = None;
            }
            return;
        }

        if let Some(character) = translate_key(event, self.modifiers) {
            self.last_replacement = None;
            self.matcher.typed(character);
            self.refresh_autocomplete();
        } else {
            self.matcher.reset();
            self.autocomplete.hide();
            self.last_replacement = None;
        }
    }

    fn handle_tab(&mut self, expected_window: isize) {
        if self.current_window_is_own_process || self.controller.is_paused() {
            self.matcher.reset();
            self.autocomplete.hide();
            replay_tab();
            return;
        }

        let Some(trigger) = self.matcher.take_trigger() else {
            self.autocomplete.hide();
            replay_tab();
            return;
        };
        let selected_completion = self.autocomplete.selected_completion(&trigger);
        self.autocomplete.hide();

        let foreground_window = unsafe { GetForegroundWindow() } as isize;
        if foreground_window != expected_window {
            replay_tab();
            return;
        }

        if let Some(expansion) = self.controller.expansion_for(&trigger) {
            let erase_count = trigger.chars().count();
            if let Err(error) = send_replacement(erase_count, &expansion) {
                tracing::warn!(%error, "failed to insert snippet");
                replay_tab();
            }
            return;
        }

        if let Some(completion) = selected_completion {
            if let Err(error) = send_replacement(0, &completion.suffix) {
                tracing::warn!(%error, "failed to insert autocomplete suggestion");
                replay_tab();
            } else {
                self.last_replacement = Some(LastReplacement {
                    original: trigger,
                    replacement: completion.word,
                    boundary: None,
                    window: expected_window,
                });
            }
            return;
        }

        if let Some(correction) = self.autocorrect.correction_for(&trigger) {
            if let Err(error) = send_replacement(trigger.chars().count(), &correction.replacement) {
                tracing::warn!(%error, "failed to insert autocorrection");
            } else {
                self.last_replacement = Some(LastReplacement {
                    original: correction.original,
                    replacement: correction.replacement,
                    boundary: Some(KeyStroke::from(KeyEvent {
                        vk_code: u32::from(VK_TAB),
                        scan_code: 0,
                        pressed: true,
                        kind: KeyEventKind::Tab,
                        foreground_window: expected_window,
                    })),
                    window: expected_window,
                });
            }
        }
        replay_tab();
    }

    fn handle_boundary(&mut self, event: KeyEvent) {
        self.autocomplete.hide();

        if let Some(character) = translate_key(event, self.modifiers)
            && is_tracked_word_character(character)
        {
            replay_key(KeyStroke::from(event));
            self.last_replacement = None;
            if self.current_window_is_own_process || self.controller.is_paused() {
                self.matcher.reset();
            } else {
                self.matcher.typed(character);
                self.refresh_autocomplete();
            }
            return;
        }

        if self.current_window_is_own_process || self.controller.is_paused() {
            self.matcher.reset();
            self.last_replacement = None;
            replay_key(KeyStroke::from(event));
            return;
        }

        let word = self.matcher.take_trigger();
        let foreground_window = unsafe { GetForegroundWindow() } as isize;
        if foreground_window != event.foreground_window {
            self.last_replacement = None;
            replay_key(KeyStroke::from(event));
            return;
        }

        let correction = word
            .as_deref()
            .and_then(|word| self.autocorrect.correction_for(word));
        if let Some(correction) = correction {
            if let Err(error) =
                send_replacement(correction.original.chars().count(), &correction.replacement)
            {
                tracing::warn!(%error, "failed to insert autocorrection");
                self.last_replacement = None;
            } else {
                self.last_replacement = Some(LastReplacement {
                    original: correction.original,
                    replacement: correction.replacement,
                    boundary: Some(KeyStroke::from(event)),
                    window: event.foreground_window,
                });
            }
        } else {
            self.last_replacement = None;
        }

        replay_key(KeyStroke::from(event));
    }

    fn handle_undo(&mut self, expected_window: isize) {
        self.autocomplete.hide();
        self.matcher.reset();

        let Some(replacement) = self.last_replacement.take() else {
            replay_key(KeyStroke {
                vk_code: u32::from(VK_Z),
                scan_code: 0,
            });
            return;
        };

        if replacement.window != expected_window
            || unsafe { GetForegroundWindow() } as isize != expected_window
        {
            replay_key(KeyStroke {
                vk_code: u32::from(VK_Z),
                scan_code: 0,
            });
            return;
        }

        if let Err(error) = undo_replacement(&replacement) {
            tracing::warn!(%error, "failed to undo TextPilot replacement");
        }
    }

    fn refresh_autocomplete(&self) {
        thread::sleep(Duration::from_millis(8));
        self.autocomplete.present_for(
            self.matcher.current(),
            self.current_window,
            &self.anchor_locator,
        );
    }
}

#[derive(Clone, Copy, Default)]
struct Modifiers {
    shift: bool,
    control: bool,
    alt: bool,
    windows: bool,
}

impl Modifiers {
    fn update(&mut self, vk_code: u32, pressed: bool) {
        match vk_code as u16 {
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT => self.shift = pressed,
            VK_CONTROL | VK_LCONTROL | VK_RCONTROL => self.control = pressed,
            VK_MENU | VK_LMENU | VK_RMENU => self.alt = pressed,
            VK_LWIN | VK_RWIN => self.windows = pressed,
            _ => {}
        }
    }

    fn command_modifier_active(self) -> bool {
        self.control || self.alt || self.windows
    }
}

fn run_hook_loop(ready_sender: mpsc::Sender<io::Result<u32>>) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let module = unsafe { GetModuleHandleW(ptr::null()) };
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0) };

    if hook.is_null() {
        let _ = ready_sender.send(Err(io::Error::last_os_error()));
        return;
    }

    let _ = ready_sender.send(Ok(thread_id));
    let mut message: MSG = unsafe { mem::zeroed() };
    while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {}

    unsafe {
        UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let data = unsafe { *(l_param as *const KBDLLHOOKSTRUCT) };
        if data.dwExtraInfo != INPUT_MARKER {
            let pressed = matches!(w_param as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let released = matches!(w_param as u32, WM_KEYUP | WM_SYSKEYUP);

            if AUTOCOMPLETE_VISIBLE.load(Ordering::Acquire)
                && matches!(data.vkCode as u16, VK_UP | VK_DOWN | VK_ESCAPE)
                && plain_navigation_pressed()
            {
                if pressed {
                    if AUTOCOMPLETE_NAV_SUPPRESSED
                        .compare_exchange(0, data.vkCode, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return 1;
                    }

                    let kind = match data.vkCode as u16 {
                        VK_UP => KeyEventKind::AutocompletePrevious,
                        VK_DOWN => KeyEventKind::AutocompleteNext,
                        _ => KeyEventKind::AutocompleteDismiss,
                    };
                    if let Some(sender) = HOOK_SENDER.get()
                        && sender
                            .try_send(KeyEvent {
                                vk_code: data.vkCode,
                                scan_code: data.scanCode,
                                pressed: true,
                                kind,
                                foreground_window: unsafe { GetForegroundWindow() } as isize,
                            })
                            .is_ok()
                    {
                        return 1;
                    }

                    AUTOCOMPLETE_NAV_SUPPRESSED.store(0, Ordering::Release);
                } else if released
                    && AUTOCOMPLETE_NAV_SUPPRESSED
                        .compare_exchange(data.vkCode, 0, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    return 1;
                }
            }

            if data.vkCode == u32::from(VK_SPACE) {
                if pressed && alt_space_pressed() {
                    if QUICK_SEARCH_SUPPRESSED
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return 1;
                    }

                    if let Some(sender) = HOOK_SENDER.get()
                        && sender
                            .try_send(KeyEvent {
                                vk_code: data.vkCode,
                                scan_code: data.scanCode,
                                pressed: true,
                                kind: KeyEventKind::QuickSearch,
                                foreground_window: unsafe { GetForegroundWindow() } as isize,
                            })
                            .is_ok()
                    {
                        return 1;
                    }

                    QUICK_SEARCH_SUPPRESSED.store(false, Ordering::Release);
                } else if released && QUICK_SEARCH_SUPPRESSED.swap(false, Ordering::AcqRel) {
                    return 1;
                }
            }

            if data.vkCode == u32::from(VK_Z) {
                if pressed && ctrl_z_pressed() {
                    if UNDO_SUPPRESSED
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return 1;
                    }

                    if let Some(sender) = HOOK_SENDER.get()
                        && sender
                            .try_send(KeyEvent {
                                vk_code: data.vkCode,
                                scan_code: data.scanCode,
                                pressed: true,
                                kind: KeyEventKind::Undo,
                                foreground_window: unsafe { GetForegroundWindow() } as isize,
                            })
                            .is_ok()
                    {
                        return 1;
                    }

                    UNDO_SUPPRESSED.store(false, Ordering::Release);
                } else if released && UNDO_SUPPRESSED.swap(false, Ordering::AcqRel) {
                    return 1;
                }
            }

            if data.vkCode == u32::from(VK_TAB) {
                if pressed && plain_tab_pressed() {
                    if TAB_SUPPRESSED
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return 1;
                    }

                    if let Some(sender) = HOOK_SENDER.get()
                        && sender
                            .try_send(KeyEvent {
                                vk_code: data.vkCode,
                                scan_code: data.scanCode,
                                pressed: true,
                                kind: KeyEventKind::Tab,
                                foreground_window: unsafe { GetForegroundWindow() } as isize,
                            })
                            .is_ok()
                    {
                        return 1;
                    }

                    TAB_SUPPRESSED.store(false, Ordering::Release);
                } else if released && TAB_SUPPRESSED.swap(false, Ordering::AcqRel) {
                    return 1;
                }
            }

            if is_autocorrect_boundary_key(data.vkCode) {
                if pressed && plain_boundary_pressed() {
                    if BOUNDARY_SUPPRESSED
                        .compare_exchange(0, data.vkCode, Ordering::AcqRel, Ordering::Acquire)
                        .is_err()
                    {
                        return 1;
                    }

                    if let Some(sender) = HOOK_SENDER.get()
                        && sender
                            .try_send(KeyEvent {
                                vk_code: data.vkCode,
                                scan_code: data.scanCode,
                                pressed: true,
                                kind: KeyEventKind::Boundary,
                                foreground_window: unsafe { GetForegroundWindow() } as isize,
                            })
                            .is_ok()
                    {
                        return 1;
                    }

                    BOUNDARY_SUPPRESSED.store(0, Ordering::Release);
                } else if released
                    && BOUNDARY_SUPPRESSED
                        .compare_exchange(data.vkCode, 0, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    return 1;
                }
            }

            if (pressed || released)
                && let Some(sender) = HOOK_SENDER.get()
            {
                let _ = sender.try_send(KeyEvent {
                    vk_code: data.vkCode,
                    scan_code: data.scanCode,
                    pressed,
                    kind: KeyEventKind::Regular,
                    foreground_window: unsafe { GetForegroundWindow() } as isize,
                });
            }
        }
    }

    unsafe { CallNextHookEx(ptr::null_mut(), code, w_param, l_param) }
}

fn translate_key(event: KeyEvent, modifiers: Modifiers) -> Option<char> {
    let mut keyboard_state = [0u8; 256];
    keyboard_state[event.vk_code as usize] = 0x80;
    if modifiers.shift {
        keyboard_state[usize::from(VK_SHIFT)] = 0x80;
    }
    if unsafe { GetKeyState(i32::from(VK_CAPITAL)) } & 1 != 0 {
        keyboard_state[usize::from(VK_CAPITAL)] = 1;
    }

    let window = event.foreground_window as *mut _;
    let thread_id = unsafe { GetWindowThreadProcessId(window, ptr::null_mut()) };
    let layout = unsafe { GetKeyboardLayout(thread_id) };
    let mut output = [0u16; 4];
    let length = unsafe {
        ToUnicodeEx(
            event.vk_code,
            event.scan_code,
            keyboard_state.as_ptr(),
            output.as_mut_ptr(),
            output.len() as i32,
            4,
            layout,
        )
    };

    (length > 0)
        .then(|| char::decode_utf16(output[..length as usize].iter().copied()).next())
        .flatten()
        .and_then(Result::ok)
}

fn plain_tab_pressed() -> bool {
    ![
        VK_SHIFT,
        VK_CONTROL,
        VK_MENU,
        VK_LWIN,
        VK_RWIN,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
    ]
    .into_iter()
    .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0)
}

fn plain_boundary_pressed() -> bool {
    ![
        VK_CONTROL,
        VK_MENU,
        VK_LWIN,
        VK_RWIN,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
    ]
    .into_iter()
    .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0)
}

fn plain_navigation_pressed() -> bool {
    ![
        VK_SHIFT,
        VK_CONTROL,
        VK_MENU,
        VK_LWIN,
        VK_RWIN,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
    ]
    .into_iter()
    .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0)
}

fn ctrl_z_pressed() -> bool {
    let control_pressed = [VK_CONTROL, VK_LCONTROL, VK_RCONTROL]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0);
    let conflicting_modifier = [VK_MENU, VK_LWIN, VK_RWIN, VK_LMENU, VK_RMENU]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0);

    control_pressed && !conflicting_modifier
}

fn alt_space_pressed() -> bool {
    let alt_pressed = [VK_MENU, VK_LMENU, VK_RMENU]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0);
    let conflicting_modifier = [VK_CONTROL, VK_LWIN, VK_RWIN, VK_LCONTROL, VK_RCONTROL]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0);

    alt_pressed && !conflicting_modifier
}

pub fn focus_window(window: isize) -> io::Result<()> {
    if window == 0 || unsafe { SetForegroundWindow(window as *mut _) } == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

pub fn insert_text_into_window(window: isize, text: &str) -> io::Result<()> {
    focus_window(window)?;
    thread::sleep(Duration::from_millis(60));
    send_replacement(0, text)
}

fn send_replacement(erase_count: usize, replacement: &str) -> io::Result<()> {
    if requires_clipboard_paste(replacement) {
        return send_clipboard_replacement(erase_count, replacement);
    }

    let inputs = replacement_inputs(erase_count, replacement);
    send_inputs(&inputs)
}

fn send_clipboard_replacement(erase_count: usize, replacement: &str) -> io::Result<()> {
    let replacement = replacement.to_owned();
    thread::Builder::new()
        .name("textpilot-clipboard-paste".into())
        .spawn(move || clipboard_paste(erase_count, &replacement))?
        .join()
        .map_err(|_| io::Error::other("clipboard paste thread panicked"))?
}

fn clipboard_paste(erase_count: usize, replacement: &str) -> io::Result<()> {
    unsafe { OleInitialize(None) }.map_err(|error| io::Error::other(error.to_string()))?;
    let _ole = OleGuard;
    let snapshot = capture_clipboard()?;

    let temporary_sequence = match set_clipboard_text(replacement) {
        Ok(sequence) => sequence,
        Err(error) => {
            let _ = restore_clipboard(snapshot);
            return Err(error);
        }
    };
    let paste_result = send_inputs(&paste_inputs(erase_count));
    thread::sleep(CLIPBOARD_PASTE_DELAY);

    let restore_result = if unsafe { GetClipboardSequenceNumber() } == temporary_sequence {
        restore_clipboard(snapshot)
    } else {
        Ok(())
    };

    paste_result.and(restore_result)
}

fn capture_clipboard() -> io::Result<ClipboardSnapshot> {
    let mut last_error = None;

    for _ in 0..CLIPBOARD_RETRY_COUNT {
        match unsafe { OleGetClipboard() } {
            Ok(data) => return Ok(ClipboardSnapshot::Data(data)),
            Err(error) => {
                unsafe {
                    SetLastError(ERROR_SUCCESS);
                }
                let format_count = unsafe { CountClipboardFormats() };
                let count_error = unsafe { GetLastError() };
                if format_count == 0 && count_error == ERROR_SUCCESS {
                    return Ok(ClipboardSnapshot::Empty);
                }
                last_error = Some(error.to_string());
                thread::sleep(CLIPBOARD_RETRY_DELAY);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        last_error.unwrap_or_else(|| "clipboard is unavailable".into()),
    ))
}

fn set_clipboard_text(text: &str) -> io::Result<u32> {
    if text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "clipboard text contains a null character",
        ));
    }

    let normalized = normalize_clipboard_text(text);
    let mut utf16 = normalized.encode_utf16().collect::<Vec<_>>();
    utf16.push(0);

    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, utf16.len() * mem::size_of::<u16>()) };
    if memory.is_null() {
        return Err(io::Error::last_os_error());
    }

    let destination = unsafe { GlobalLock(memory) }.cast::<u16>();
    if destination.is_null() {
        unsafe {
            GlobalFree(memory);
        }
        return Err(io::Error::last_os_error());
    }

    unsafe {
        ptr::copy_nonoverlapping(utf16.as_ptr(), destination, utf16.len());
        GlobalUnlock(memory);
    }

    let _clipboard = match open_clipboard() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            unsafe {
                GlobalFree(memory);
            }
            return Err(error);
        }
    };

    if unsafe { EmptyClipboard() } == 0 {
        unsafe {
            GlobalFree(memory);
        }
        return Err(io::Error::last_os_error());
    }

    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT.0), memory) }.is_null() {
        unsafe {
            GlobalFree(memory);
        }
        return Err(io::Error::last_os_error());
    }

    Ok(unsafe { GetClipboardSequenceNumber() })
}

fn restore_clipboard(snapshot: ClipboardSnapshot) -> io::Result<()> {
    match snapshot {
        ClipboardSnapshot::Data(data) => unsafe { OleSetClipboard(&data) }
            .and_then(|_| unsafe { OleFlushClipboard() })
            .map_err(|error| io::Error::other(error.to_string())),
        ClipboardSnapshot::Empty => {
            let _clipboard = open_clipboard()?;
            if unsafe { EmptyClipboard() } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }
}

fn open_clipboard() -> io::Result<OpenClipboardGuard> {
    for _ in 0..CLIPBOARD_RETRY_COUNT {
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            return Ok(OpenClipboardGuard);
        }
        thread::sleep(CLIPBOARD_RETRY_DELAY);
    }

    Err(io::Error::last_os_error())
}

fn requires_clipboard_paste(text: &str) -> bool {
    text.contains(['\r', '\n'])
}

fn normalize_clipboard_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                normalized.push_str("\r\n");
            }
            '\n' => normalized.push_str("\r\n"),
            _ => normalized.push(character),
        }
    }

    normalized
}

fn paste_inputs(erase_count: usize) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(erase_count * 2 + 4);

    for _ in 0..erase_count {
        inputs.push(key_input(VK_BACK, 0, 0));
        inputs.push(key_input(VK_BACK, 0, KEYEVENTF_KEYUP));
    }

    inputs.push(key_input(VK_CONTROL, 0, 0));
    inputs.push(key_input(b'V' as u16, 0, 0));
    inputs.push(key_input(b'V' as u16, 0, KEYEVENTF_KEYUP));
    inputs.push(key_input(VK_CONTROL, 0, KEYEVENTF_KEYUP));
    inputs
}

fn replay_tab() {
    let inputs = [
        key_input(VK_TAB, 0, 0),
        key_input(VK_TAB, 0, KEYEVENTF_KEYUP),
    ];
    if let Err(error) = send_inputs(&inputs) {
        tracing::warn!(%error, "failed to replay Tab");
    }
}

fn replay_key(key: KeyStroke) {
    let inputs = [
        key_input(key.vk_code as u16, key.scan_code as u16, 0),
        key_input(key.vk_code as u16, key.scan_code as u16, KEYEVENTF_KEYUP),
    ];
    if let Err(error) = send_inputs(&inputs) {
        tracing::warn!(%error, "failed to replay key");
    }
}

fn replay_suppressed_event(event: KeyEvent) {
    match event.kind {
        KeyEventKind::Regular => {}
        KeyEventKind::Tab => replay_tab(),
        KeyEventKind::Boundary
        | KeyEventKind::Undo
        | KeyEventKind::QuickSearch
        | KeyEventKind::AutocompletePrevious
        | KeyEventKind::AutocompleteNext
        | KeyEventKind::AutocompleteDismiss => replay_key(KeyStroke::from(event)),
    }
}

fn undo_replacement(replacement: &LastReplacement) -> io::Result<()> {
    let boundary_length = usize::from(replacement.boundary.is_some());
    let mut inputs = Vec::new();
    inputs.push(key_input(VK_CONTROL, 0, KEYEVENTF_KEYUP));
    inputs.extend(replacement_inputs(
        replacement.replacement.chars().count() + boundary_length,
        &replacement.original,
    ));

    if let Some(boundary) = replacement.boundary {
        inputs.push(key_input(
            boundary.vk_code as u16,
            boundary.scan_code as u16,
            0,
        ));
        inputs.push(key_input(
            boundary.vk_code as u16,
            boundary.scan_code as u16,
            KEYEVENTF_KEYUP,
        ));
    }

    send_inputs(&inputs)
}

fn send_inputs(inputs: &[INPUT]) -> io::Result<()> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };

    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn replacement_inputs(erase_count: usize, replacement: &str) -> Vec<INPUT> {
    let text_units = replacement.encode_utf16().count();
    let mut inputs = Vec::with_capacity((erase_count + text_units) * 2);

    for _ in 0..erase_count {
        inputs.push(key_input(VK_BACK, 0, 0));
        inputs.push(key_input(VK_BACK, 0, KEYEVENTF_KEYUP));
    }

    for unit in replacement.encode_utf16() {
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }

    inputs
}

fn key_input(virtual_key: u16, scan_code: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INPUT_MARKER,
            },
        },
    }
}

fn is_navigation_key(vk_code: u32) -> bool {
    matches!(
        vk_code as u16,
        VK_DELETE
            | VK_LEFT
            | VK_RIGHT
            | VK_UP
            | VK_DOWN
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_ESCAPE
            | VK_RETURN
    )
}

fn is_modifier_key(vk_code: u32) -> bool {
    matches!(
        vk_code as u16,
        VK_SHIFT
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_CONTROL
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_MENU
            | VK_LMENU
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
    )
}

fn is_autocorrect_boundary_key(vk_code: u32) -> bool {
    matches!(
        vk_code as u16,
        VK_SPACE
            | VK_RETURN
            | VK_OEM_1
            | VK_OEM_2
            | VK_OEM_3
            | VK_OEM_4
            | VK_OEM_5
            | VK_OEM_6
            | VK_OEM_7
            | VK_OEM_COMMA
            | VK_OEM_PERIOD
    )
}

fn is_tracked_word_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}

fn window_process_id(window: isize) -> u32 {
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(window as *mut _, &mut process_id);
    }
    process_id
}

fn suggestion_screen_position(
    window: isize,
    anchor_locator: &InputAnchorLocator,
) -> SuggestionAnchor {
    caret_screen_position(window)
        .or_else(|| anchor_locator.focused_input_anchor())
        .or_else(|| window_anchor(window))
        .unwrap_or(SuggestionAnchor {
            x: 24,
            top: 24,
            bottom: 42,
        })
}

fn quick_search_screen_position(
    window: isize,
    anchor_locator: &InputAnchorLocator,
) -> Option<(i32, i32)> {
    caret_screen_position(window)
        .or_else(|| anchor_locator.focused_input_anchor())
        .map(|anchor| (anchor.x, anchor.bottom))
}

fn caret_screen_position(window: isize) -> Option<SuggestionAnchor> {
    let thread_id = unsafe { GetWindowThreadProcessId(window as *mut _, ptr::null_mut()) };
    let mut info = GUITHREADINFO {
        cbSize: mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };

    if unsafe { GetGUIThreadInfo(thread_id, &mut info) } == 0 || info.hwndCaret.is_null() {
        return None;
    }

    let mut top = POINT {
        x: info.rcCaret.left,
        y: info.rcCaret.top,
    };
    let mut bottom = POINT {
        x: info.rcCaret.left,
        y: info.rcCaret.bottom,
    };
    if unsafe { ClientToScreen(info.hwndCaret, &mut top) } == 0
        || unsafe { ClientToScreen(info.hwndCaret, &mut bottom) } == 0
    {
        return None;
    }

    Some(SuggestionAnchor {
        x: bottom.x,
        top: top.y,
        bottom: bottom.y,
    })
}

fn window_anchor(window: isize) -> Option<SuggestionAnchor> {
    let mut rectangle = RECT::default();
    if unsafe { GetWindowRect(window as *mut _, &mut rectangle) } == 0 {
        return None;
    }

    anchor_from_rect(
        rectangle.left + 20,
        rectangle.top + 20,
        rectangle.right,
        rectangle.bottom,
    )
}

fn normalize_trigger(trigger: &str) -> String {
    trigger.to_lowercase()
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, KEYEVENTF_KEYUP, VK_BACK, VK_CONTROL,
    };

    use super::{
        SnippetController, normalize_clipboard_text, paste_inputs, replacement_inputs,
        requires_clipboard_paste,
    };

    fn keyboard_data(input: &INPUT) -> (u16, u16, u32) {
        let keyboard = unsafe { input.Anonymous.ki };
        (keyboard.wVk, keyboard.wScan, keyboard.dwFlags)
    }

    #[test]
    fn snippet_lookup_is_case_insensitive() {
        let controller = SnippetController::new(vec![("КП".into(), "Предложение".into())]);

        assert_eq!(
            controller.expansion_for("кп").as_deref(),
            Some("Предложение")
        );
    }

    #[test]
    fn replacement_contains_backspaces_and_unicode_pairs_without_delimiter() {
        let inputs = replacement_inputs(2, "Да");

        assert_eq!(inputs.len(), 8);
    }

    #[test]
    fn multiline_replacements_use_clipboard_paste() {
        assert!(requires_clipboard_paste("first\nsecond"));
        assert!(requires_clipboard_paste("first\r\nsecond"));
        assert!(!requires_clipboard_paste("one line"));
    }

    #[test]
    fn clipboard_text_normalizes_line_endings_to_crlf() {
        assert_eq!(
            normalize_clipboard_text("one\ntwo\rthree\r\nfour"),
            "one\r\ntwo\r\nthree\r\nfour"
        );
    }

    #[test]
    fn clipboard_paste_erases_trigger_then_sends_ctrl_v() {
        let inputs = paste_inputs(1);

        assert_eq!(inputs.len(), 6);
        assert_eq!(keyboard_data(&inputs[0]), (VK_BACK, 0, 0));
        assert_eq!(keyboard_data(&inputs[1]), (VK_BACK, 0, KEYEVENTF_KEYUP));
        assert_eq!(keyboard_data(&inputs[2]), (VK_CONTROL, 0, 0));
        assert_eq!(keyboard_data(&inputs[3]), (b'V' as u16, 0, 0));
        assert_eq!(keyboard_data(&inputs[4]), (b'V' as u16, 0, KEYEVENTF_KEYUP));
        assert_eq!(keyboard_data(&inputs[5]), (VK_CONTROL, 0, KEYEVENTF_KEYUP));
    }
}
