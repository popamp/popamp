//! Dialogs on Windows: the shell's file dialogs, message boxes, and a small
//! form (a label, a field and OK / Cancel) for text entry and lists, built
//! as a dialog template in memory.

/// A control's window class, by the atom the system gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Button = 0x80,
    Edit = 0x81,
    Static = 0x82,
    ListBox = 0x83,
}

/// A control in a form, placed in dialog units.
#[derive(Debug, Clone)]
struct Control {
    class: Class,
    style: u32,
    ex_style: u32,
    rect: [i16; 4],
    id: u16,
    text: String,
}

// Styles and ids from winuser.h, kept here so the form is built (and
// tested) without the system.
const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_EX_CLIENTEDGE: u32 = 0x200;
const DS_SETFONT: u32 = 0x40;
const DS_MODALFRAME: u32 = 0x80;
const DS_CENTER: u32 = 0x800;
const BS_DEFPUSHBUTTON: u32 = 1;
const ES_AUTOHSCROLL: u32 = 0x80;
const LBS_NOTIFY: u32 = 1;
const LBS_NOINTEGRALHEIGHT: u32 = 0x100;
const SS_NOPREFIX: u32 = 0x80;
const IDOK: u16 = 1;
const IDCANCEL: u16 = 2;
/// The form's field: the text box or the list.
const FIELD: u16 = 100;

const MARGIN: i16 = 7;
const LABEL_H: i16 = 9;
const BUTTON: (i16, i16) = (50, 14);

/// What a form asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    /// One line of text.
    Text,
    /// One of a list's rows, this many dialog units tall.
    List(i16),
}

/// A form `width` dialog units wide: `label` over the field, OK and Cancel
/// under it on the right. Returns the form's size and its controls.
fn form(width: i16, label: &str, field: Field) -> ((i16, i16), Vec<Control>) {
    let inner = width - 2 * MARGIN;
    let field_y = MARGIN + LABEL_H + 2;
    let (class, style, field_h) = match field {
        Field::Text => (Class::Edit, ES_AUTOHSCROLL, 13),
        Field::List(h) => (Class::ListBox, LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | WS_VSCROLL, h),
    };
    let buttons_y = field_y + field_h + MARGIN;
    let button = |x: i16, id: u16, style: u32, text: &str| Control {
        class: Class::Button,
        style: style | WS_TABSTOP,
        ex_style: 0,
        rect: [x, buttons_y, BUTTON.0, BUTTON.1],
        id,
        text: text.into(),
    };
    let cancel_x = width - MARGIN - BUTTON.0;
    let controls = vec![
        Control {
            class: Class::Static,
            style: SS_NOPREFIX,
            ex_style: 0,
            rect: [MARGIN, MARGIN, inner, LABEL_H],
            id: 0xFFFF,
            text: label.into(),
        },
        Control {
            class,
            style: style | WS_TABSTOP,
            ex_style: WS_EX_CLIENTEDGE,
            rect: [MARGIN, field_y, inner, field_h],
            id: FIELD,
            text: String::new(),
        },
        button(cancel_x - 4 - BUTTON.0, IDOK, BS_DEFPUSHBUTTON, "OK"),
        button(cancel_x, IDCANCEL, 0, "Cancel"),
    ];
    ((width, buttons_y + BUTTON.1 + MARGIN), controls)
}

/// A dialog template (DLGTEMPLATE and its DLGITEMTEMPLATEs) as words:
/// each control starts on a double word, as the system requires.
fn template(title: &str, (cx, cy): (i16, i16), controls: &[Control]) -> Vec<u16> {
    let mut w: Vec<u16> = Vec::new();
    let dword = |w: &mut Vec<u16>, v: u32| w.extend([v as u16, (v >> 16) as u16]);
    let text = |w: &mut Vec<u16>, s: &str| w.extend(s.encode_utf16().chain([0]));
    dword(&mut w, WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_SETFONT | DS_MODALFRAME | DS_CENTER);
    dword(&mut w, 0);
    w.extend([controls.len() as u16, 0, 0, cx as u16, cy as u16]);
    w.extend([0, 0]); // no menu, the standard class
    text(&mut w, title);
    w.push(8);
    text(&mut w, "MS Shell Dlg");
    for c in controls {
        if w.len() % 2 == 1 {
            w.push(0);
        }
        dword(&mut w, c.style | WS_CHILD | WS_VISIBLE);
        dword(&mut w, c.ex_style);
        w.extend(c.rect.map(|v| v as u16));
        w.extend([c.id, 0xFFFF, c.class as u16]);
        text(&mut w, &c.text);
        w.push(0); // no creation data
    }
    w
}

#[cfg(windows)]
pub use system::execute;

#[cfg(windows)]
mod system {
    use super::*;
    use crate::dialogs::Request;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, SHCreateItemFromParsingName,
        FILEOPENDIALOGOPTIONS, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DialogBoxIndirectParamW, EndDialog, GetDlgItemTextW, MessageBoxW, SendDlgItemMessageW, SetDlgItemTextW, DLGTEMPLATE, IDYES,
        LBN_DBLCLK, LB_ADDSTRING, LB_ERR, LB_GETCURSEL, LB_SETCURSEL, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_SETFOREGROUND,
        MB_YESNO, WM_COMMAND, WM_INITDIALOG,
    };

    /// Runs `req`; the chosen values, or none when cancelled.
    pub fn execute(req: &Request) -> Vec<String> {
        let _com = Com::start();
        let shown = match req {
            Request::Files { title, multiple, filter, start } => {
                let more = if *multiple { FOS_ALLOWMULTISELECT } else { FILEOPENDIALOGOPTIONS(0) };
                open(title, more | FOS_FILEMUSTEXIST, filter, start.as_deref())
            }
            Request::Folder { title, multiple, start } => {
                let more = if *multiple { FOS_ALLOWMULTISELECT } else { FILEOPENDIALOGOPTIONS(0) };
                open(title, more | FOS_PICKFOLDERS, &[], start.as_deref())
            }
            Request::Save { title, name, start } => save(title, name, start.as_deref()),
            Request::Entry { title, text, initial } => Ok(ask(title, text, Field::Text, Form::Text(initial.clone()))),
            Request::List { title, column, rows } => Ok(ask(title, column, Field::List(176), Form::List(rows.clone()))),
            Request::Info { title, text } => {
                message(title, text, MB_OK | MB_ICONINFORMATION);
                Ok(vec![])
            }
            Request::Question { title, text } => {
                Ok(if message(title, text, MB_YESNO | MB_ICONQUESTION) == IDYES { vec!["yes".into()] } else { vec![] })
            }
        };
        shown.unwrap_or_else(|e| {
            if e.code() != ERROR_CANCELLED.to_hresult() {
                log::warn!("dialog failed: {e}");
            }
            vec![]
        })
    }

    /// COM in a single-threaded apartment, as the shell's dialogs want,
    /// for the life of one dialog's thread.
    struct Com(bool);

    impl Com {
        fn start() -> Com {
            // SAFETY: this thread is new and ends with the dialog.
            Com(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.is_ok())
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the CoInitializeEx above.
                unsafe { CoUninitialize() };
            }
        }
    }

    fn message(
        title: &str,
        text: &str,
        style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
    ) -> windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT {
        // SAFETY: both strings live through the call.
        unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), style | MB_SETFOREGROUND) }
    }

    /// Titles a file dialog, opens it at `start` and adds `options`.
    unsafe fn prepare(
        dialog: &IFileDialog,
        title: &str,
        options: FILEOPENDIALOGOPTIONS,
        start: Option<&Path>,
    ) -> windows::core::Result<()> {
        // SAFETY: calls on a live dialog with strings that outlive them.
        unsafe {
            dialog.SetTitle(&HSTRING::from(title))?;
            dialog.SetOptions(dialog.GetOptions()? | options | FOS_FORCEFILESYSTEM)?;
            if let Some(folder) = start.and_then(|s| SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(s), None).ok()) {
                dialog.SetFolder(&folder)?;
            }
        }
        Ok(())
    }

    fn open(title: &str, options: FILEOPENDIALOGOPTIONS, filter: &[String], start: Option<&Path>) -> windows::core::Result<Vec<String>> {
        let (media, all) = (HSTRING::from(filter.join(";")), HSTRING::from("*.*"));
        let (media_name, all_name) = (HSTRING::from("Media files"), HSTRING::from("All files"));
        let types = [
            COMDLG_FILTERSPEC { pszName: PCWSTR(media_name.as_ptr()), pszSpec: PCWSTR(media.as_ptr()) },
            COMDLG_FILTERSPEC { pszName: PCWSTR(all_name.as_ptr()), pszSpec: PCWSTR(all.as_ptr()) },
        ];
        // SAFETY: COM calls on a dialog created here; the filter strings
        // live until it is gone.
        unsafe {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            prepare(&dialog.cast()?, title, options, start)?;
            if !filter.is_empty() {
                dialog.SetFileTypes(&types)?;
            }
            dialog.Show(None)?;
            let items = dialog.GetResults()?;
            (0..items.GetCount()?).map(|i| path(&items.GetItemAt(i)?)).collect()
        }
    }

    fn save(title: &str, name: &str, start: Option<&Path>) -> windows::core::Result<Vec<String>> {
        // SAFETY: COM calls on a dialog created here.
        unsafe {
            let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
            prepare(&dialog.cast()?, title, FOS_OVERWRITEPROMPT, start)?;
            dialog.SetFileName(&HSTRING::from(name))?;
            if let Some(ext) = Path::new(name).extension() {
                dialog.SetDefaultExtension(&HSTRING::from(ext))?;
            }
            dialog.Show(None)?;
            Ok(vec![path(&dialog.GetResult()?)?])
        }
    }

    fn path(item: &IShellItem) -> windows::core::Result<String> {
        // SAFETY: the shell allocates the name; we free it once, copied.
        unsafe {
            let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let out = name.to_string();
            CoTaskMemFree(Some(name.0 as *const _));
            Ok(PathBuf::from(out.map_err(|_| windows::core::Error::empty())?).display().to_string())
        }
    }

    /// What a form shows.
    enum Form {
        /// A text box, holding this to start with.
        Text(String),
        /// A list of these rows.
        List(Vec<String>),
    }

    thread_local! {
        /// The form this thread shows, and what was chosen in it: one at a
        /// time, as each dialog has a thread of its own.
        static FORM: RefCell<Option<Form>> = const { RefCell::new(None) };
        static CHOSEN: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Shows a form and returns its value: the text, or the chosen row's
    /// index.
    fn ask(title: &str, label: &str, field: Field, shown: Form) -> Vec<String> {
        let width = if matches!(field, Field::Text) { 260 } else { 240 };
        let (size, controls) = form(width, label, field);
        let words = template(title, size, &controls);
        // The template, on a double-word boundary.
        let aligned: Vec<u32> = words.chunks(2).map(|c| c[0] as u32 | (c.get(1).copied().unwrap_or(0) as u32) << 16).collect();
        FORM.set(Some(shown));
        CHOSEN.set(None);
        // SAFETY: a well-formed template that outlives the modal call.
        unsafe { DialogBoxIndirectParamW(None, aligned.as_ptr().cast::<DLGTEMPLATE>(), None, Some(proc), LPARAM(0)) };
        FORM.set(None);
        CHOSEN.take().filter(|v| !v.is_empty()).into_iter().collect()
    }

    /// Sends a message to the form's field.
    fn send(dlg: HWND, msg: u32, wp: usize, lp: isize) -> isize {
        // SAFETY: a message to a control of a live dialog.
        unsafe { SendDlgItemMessageW(dlg, FIELD as i32, msg, WPARAM(wp), LPARAM(lp)) }.0
    }

    /// Puts what the form starts with in its field.
    fn fill(dlg: HWND, form: &Form) {
        match form {
            Form::Text(initial) => {
                // SAFETY: the form's text box, and a string that outlives the call.
                let _ = unsafe { SetDlgItemTextW(dlg, FIELD as i32, &HSTRING::from(initial.as_str())) };
            }
            Form::List(rows) => {
                for row in rows {
                    let row = HSTRING::from(row.as_str());
                    send(dlg, LB_ADDSTRING, 0, row.as_ptr() as isize);
                }
                send(dlg, LB_SETCURSEL, 0, 0);
            }
        }
    }

    /// The field's value: the text, or the chosen row's index.
    fn value(dlg: HWND, form: &Form) -> Option<String> {
        match form {
            Form::Text(_) => {
                let mut buf = vec![0u16; 32768];
                // SAFETY: the form's text box, read into `buf`.
                let n = unsafe { GetDlgItemTextW(dlg, FIELD as i32, &mut buf) } as usize;
                Some(String::from_utf16_lossy(&buf[..n]))
            }
            Form::List(_) => Some(send(dlg, LB_GETCURSEL, 0, 0)).filter(|&i| i != LB_ERR as isize).map(|i| i.to_string()),
        }
    }

    unsafe extern "system" fn proc(dlg: HWND, msg: u32, wp: WPARAM, _: LPARAM) -> isize {
        match msg {
            WM_INITDIALOG => {
                FORM.with_borrow(|f| f.iter().for_each(|f| fill(dlg, f)));
                1 // focus the first field
            }
            WM_COMMAND => {
                let (id, code) = ((wp.0 & 0xFFFF) as u16, ((wp.0 >> 16) & 0xFFFF) as u32);
                let done = id == IDOK || id == IDCANCEL || (id == FIELD && code == LBN_DBLCLK);
                if done {
                    if id != IDCANCEL {
                        CHOSEN.set(FORM.with_borrow(|f| f.as_ref().and_then(|f| value(dlg, f))));
                    }
                    // SAFETY: ends this dialog's modal loop.
                    let _ = unsafe { EndDialog(dlg, id as isize) };
                }
                done as isize
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A template as the system reads it.
    struct Parsed {
        title: String,
        font: String,
        /// Each control's id, class atom and text.
        controls: Vec<(u16, u16, String)>,
        /// Where each control starts, in words.
        offsets: Vec<usize>,
    }

    fn read(w: &[u16]) -> Parsed {
        fn text(w: &[u16], at: &mut usize) -> String {
            let end = *at + w[*at..].iter().position(|&u| u == 0).unwrap();
            let s = String::from_utf16(&w[*at..end]).unwrap();
            *at = end + 1;
            s
        }
        let count = w[4] as usize;
        let mut at = 9 + 2; // header, then no menu and the standard class
        let title = text(w, &mut at);
        at += 1; // point size
        let font = text(w, &mut at);
        let (mut controls, mut offsets) = (Vec::new(), Vec::new());
        for _ in 0..count {
            at += at % 2;
            offsets.push(at);
            let (id, atom) = (w[at + 8], w[at + 10]);
            assert_eq!(w[at + 9], 0xFFFF, "a class by its atom");
            at += 11;
            controls.push((id, atom, text(w, &mut at)));
            assert_eq!(w[at], 0, "no creation data");
            at += 1;
        }
        assert_eq!(at, w.len());
        Parsed { title, font, controls, offsets }
    }

    #[test]
    fn a_template_lists_its_controls_each_on_a_double_word() {
        let (size, controls) = form(260, "Location:", Field::Text);
        let w = template("Open URL", size, &controls);
        let Parsed { title, font, controls: read_back, offsets } = read(&w);
        assert_eq!((title.as_str(), font.as_str()), ("Open URL", "MS Shell Dlg"));
        assert_eq!(
            read_back,
            [
                (0xFFFF, Class::Static as u16, "Location:".into()),
                (FIELD, Class::Edit as u16, String::new()),
                (IDOK, Class::Button as u16, "OK".into()),
                (IDCANCEL, Class::Button as u16, "Cancel".into()),
            ]
        );
        assert!(offsets.iter().all(|o| o % 2 == 0), "{offsets:?}");
        assert_eq!((w[7], w[8]), (size.0 as u16, size.1 as u16));
    }

    #[test]
    fn a_form_keeps_its_controls_inside_and_apart() {
        for field in [Field::Text, Field::List(176)] {
            let ((cx, cy), controls) = form(240, "x", field);
            for c in &controls {
                let [x, y, w, h] = c.rect;
                assert!(x >= MARGIN && y >= MARGIN && x + w <= cx - MARGIN && y + h <= cy - MARGIN, "{c:?} in {cx}x{cy}");
            }
            for (i, a) in controls.iter().enumerate() {
                for b in &controls[i + 1..] {
                    let ([ax, ay, aw, ah], [bx, by, bw, bh]) = (a.rect, b.rect);
                    assert!(ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay, "{a:?} overlaps {b:?}");
                }
            }
            let field_control = controls.iter().find(|c| c.id == FIELD).unwrap();
            assert_eq!(field_control.class, if field == Field::Text { Class::Edit } else { Class::ListBox });
        }
    }
}
