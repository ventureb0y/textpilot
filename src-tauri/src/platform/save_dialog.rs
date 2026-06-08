use std::{ffi::c_void, path::PathBuf};

use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FILEOPENDIALOGOPTIONS, FOS_FORCEFILESYSTEM,
            FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FileSaveDialog, IFileSaveDialog,
            SIGDN_FILESYSPATH,
        },
    },
    core::{PCWSTR, w},
};

const CANCELLED_HRESULT: i32 = -2147023673;

pub fn choose_json_save_path(
    owner: HWND,
    suggested_file_name: &str,
) -> Result<Option<PathBuf>, String> {
    let suggested_file_name = wide_string(suggested_file_name);

    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        initialized
            .ok()
            .map_err(|error| format!("Не удалось открыть диалог сохранения: {error}"))?;
        let _com = ComApartment;

        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("Не удалось создать диалог сохранения: {error}"))?;
        dialog
            .SetTitle(w!("Экспорт конфигурации TextPilot"))
            .map_err(|error| error.to_string())?;
        dialog
            .SetFileName(PCWSTR(suggested_file_name.as_ptr()))
            .map_err(|error| error.to_string())?;
        dialog
            .SetDefaultExtension(w!("json"))
            .map_err(|error| error.to_string())?;
        dialog
            .SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: w!("JSON-файлы"),
                pszSpec: w!("*.json"),
            }])
            .map_err(|error| error.to_string())?;

        let options = dialog.GetOptions().map_err(|error| error.to_string())?;
        dialog
            .SetOptions(FILEOPENDIALOGOPTIONS(
                options.0 | FOS_OVERWRITEPROMPT.0 | FOS_PATHMUSTEXIST.0 | FOS_FORCEFILESYSTEM.0,
            ))
            .map_err(|error| error.to_string())?;

        if let Err(error) = dialog.Show(Some(owner)) {
            if error.code().0 == CANCELLED_HRESULT {
                return Ok(None);
            }
            return Err(format!("Не удалось открыть диалог сохранения: {error}"));
        }

        let item = dialog.GetResult().map_err(|error| error.to_string())?;
        let path = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| error.to_string())?;
        let path_string = path
            .to_string()
            .map_err(|error| format!("Некорректный путь сохранения: {error}"))?;
        CoTaskMemFree(Some(path.as_ptr().cast::<c_void>()));

        Ok(Some(PathBuf::from(path_string)))
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
