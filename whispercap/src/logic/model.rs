use super::transcribe::{picker_directory, picker_file};
use crate::{
    db::{
        self,
        def::{ModelEntry, MODEL_TABLE as DB_TABLE},
    },
    global_logic, global_util,
    logic::{toast, tr::tr},
    slint_generatedAppWindow::{
        AppWindow, ModelEntry as UIModelEntry, ModelSource, ModelStatus, PopupActionEntry,
    },
    toast_success,
};
use log::trace;
use once_cell::sync::Lazy;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use transcribe::whisper_model_downloader::{self, ModelDownloader};
use uuid::Uuid;

static CANCEL_SIGS: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::default()));

#[macro_export]
macro_rules! store_model_entries {
    ($ui:expr) => {
        crate::global_store!($ui)
            .get_model_entries()
            .as_any()
            .downcast_ref::<VecModel<UIModelEntry>>()
            .expect("We know we set a VecModel<UIModelEntry> earlier")
    };
}

pub fn init(ui: &AppWindow) {
    inner_init(ui);

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_available_models(move || available_models(&ui_weak.unwrap()));

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_show_undownload_models(move || show_undownload_models(&ui_weak.unwrap()));

    let ui_weak = ui.as_weak();
    global_logic!(ui)
        .on_model_statistics(move |entries| model_statistics(&ui_weak.unwrap(), entries));

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_download_model(move |model_name| {
        download_model(&ui_weak.unwrap(), model_name);
    });

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_redownload_model(move |index| {
        redownload_model(&ui_weak.unwrap(), index);
    });

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_import_model(move || {
        import_model(&ui_weak.unwrap());
    });

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_manual_download_model(move || {
        manual_download_model(&ui_weak.unwrap());
    });

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_cancel_download_model(move |index| {
        cancel_download_model(&ui_weak.unwrap(), index);
    });

    let ui_weak = ui.as_weak();
    global_logic!(ui).on_remove_model(move |index| {
        remove_model(&ui_weak.unwrap(), index);
    });
}

fn inner_init(ui: &AppWindow) {
    store_model_entries!(ui).set_vec(vec![]);

    let ui = ui.as_weak();
    tokio::spawn(async move {
        let entries = match db::entry::select_all(DB_TABLE).await {
            Ok(items) => items
                .into_iter()
                .filter_map(|item| serde_json::from_str::<ModelEntry>(&item.data).ok())
                .collect(),
            Err(e) => {
                log::warn!("{:?}", e);
                vec![]
            }
        };

        _ = slint::invoke_from_event_loop(move || {
            let ui = ui.unwrap();

            let entries = entries
                .into_iter()
                .map(|entry| {
                    let mut entry: UIModelEntry = entry.into();
                    if !PathBuf::from_str(&entry.file_path)
                        .unwrap_or_default()
                        .exists()
                    {
                        entry.status = ModelStatus::NoFound;
                    }
                    entry
                })
                .collect::<Vec<UIModelEntry>>();

            store_model_entries!(ui).set_vec(entries);
        });
    });
}

fn available_models(ui: &AppWindow) -> ModelRc<SharedString> {
    let mut seen = HashSet::new();

    let items = store_model_entries!(ui)
        .iter()
        .filter_map(|entry| {
            if !seen.insert(entry.name.clone()) {
                None
            } else {
                match entry.source {
                    ModelSource::Network => {
                        if matches!(entry.status, ModelStatus::DownloadFinished) {
                            Some(entry.name.clone())
                        } else {
                            None
                        }
                    }
                    ModelSource::Local => {
                        if matches!(entry.status, ModelStatus::Import) {
                            Some(entry.name.clone())
                        } else {
                            None
                        }
                    }
                }
            }
        })
        .collect::<Vec<SharedString>>();

    ModelRc::new(VecModel::from_iter(items.into_iter()))
}

fn show_undownload_models(ui: &AppWindow) -> ModelRc<PopupActionEntry> {
    let items: VecModel<PopupActionEntry> = VecModel::default();

    let entries = store_model_entries!(ui)
        .iter()
        .filter_map(|entry| match entry.source {
            ModelSource::Network => Some(entry.name.clone().into()),
            _ => None,
        })
        .collect::<Vec<String>>();

    for name in whisper_model_downloader::MODEL_NAMES {
        if !entries.contains(&name.to_string()) {
            items.push(PopupActionEntry {
                icon: global_logic!(ui).invoke_download_icon(),
                text: name.to_string().into(),
                action: "download-model".to_string().into(),
                user_data: name.to_string().into(),
            });
        }
    }

    ModelRc::new(items)
}

fn model_statistics(_ui: &AppWindow, entries: ModelRc<UIModelEntry>) -> ModelRc<i32> {
    let mut statistics = [0; 3];

    for entry in entries.iter() {
        match entry.source {
            ModelSource::Network => statistics[1] += 1,
            ModelSource::Local => statistics[2] += 1,
        }

        statistics[0] += 1;
    }

    ModelRc::new(VecModel::from_slice(&statistics))
}

fn download_model(ui: &AppWindow, model_name: SharedString) {
    async_download_model(ui, None, model_name);
}

fn redownload_model(ui: &AppWindow, index: i32) {
    let model_name = store_model_entries!(ui)
        .row_data(index as usize)
        .unwrap()
        .name
        .clone();

    async_download_model(ui, Some(index), model_name);
}

fn async_download_model(ui: &AppWindow, index: Option<i32>, model_name: SharedString) {
    let ui_weak = ui.as_weak();

    let id = if index.is_none() {
        Uuid::new_v4().to_string()
    } else {
        store_model_entries!(ui)
            .row_data(index.unwrap() as usize)
            .unwrap()
            .id
            .to_string()
    };

    tokio::spawn(async move {
        let Some(dir) = picker_directory(ui_weak.clone(), &tr("Save Model"), "") else {
            return;
        };

        let downloader = ModelDownloader::new(
            model_name.clone().to_string(),
            dir.to_string_lossy().to_string(),
        );

        let file_path = downloader.model_path();
        let cancel_sig = add_cancel_sig(id.clone());
        let (ui, id_duplicate, model_name_duplicate) =
            (ui_weak.clone(), id.clone(), model_name.clone());

        _ = slint::invoke_from_event_loop(move || {
            let ui = ui.unwrap();
            let entry = UIModelEntry {
                id: id_duplicate.into(),
                name: model_name_duplicate,
                file_path: file_path.into(),
                file_size: Default::default(),
                source: ModelSource::Network,
                status: ModelStatus::Downloading,
                progress: 0.0,
            };

            if index.is_none() {
                store_model_entries!(ui).push(entry.clone());
                add_db_entry(&ui, entry.into());
            } else {
                store_model_entries!(ui).set_row_data(index.unwrap() as usize, entry.clone());
                update_db_entry(&ui, entry.into());
            }
        });

        let (ui_weak_duplicate, id_duplicate) = (ui_weak.clone(), id.clone());
        match downloader
            .download_model(cancel_sig, move |downloaded, total_size, progress| {
                trace!("{model_name}: {downloaded}/{total_size} => {progress:.2}%");

                let (ui, id) = (ui_weak_duplicate.clone(), id_duplicate.clone());
                _ = slint::invoke_from_event_loop(move || {
                    let ui = ui.unwrap();

                    for (index, mut entry) in store_model_entries!(ui).iter().enumerate() {
                        if entry.id != id {
                            continue;
                        }

                        if entry.file_size.is_empty() {
                            entry.file_size = cutil::str::pretty_size_string(total_size).into();
                        }

                        entry.progress = progress / 100.0;
                        store_model_entries!(ui).set_row_data(index, entry);
                        return;
                    }
                });
            })
            .await
        {
            Ok(status) => {
                let (ui, id) = (ui_weak.clone(), id.clone());
                _ = slint::invoke_from_event_loop(move || {
                    let ui = ui.unwrap();

                    for (index, mut entry) in store_model_entries!(ui).iter().enumerate() {
                        if entry.id != id {
                            continue;
                        }

                        entry.status = status.into();
                        store_model_entries!(ui).set_row_data(index, entry.clone());
                        update_db_entry(&ui, entry.into());
                        return;
                    }
                });
            }
            Err(e) => {
                toast::async_toast_warn(
                    ui_weak.clone(),
                    format!("{}. {e}", tr("Download model failed")),
                );

                let (ui, id) = (ui_weak.clone(), id.clone());
                _ = slint::invoke_from_event_loop(move || {
                    let ui = ui.unwrap();

                    for (index, mut entry) in store_model_entries!(ui).iter().enumerate() {
                        if entry.id != id {
                            continue;
                        }

                        entry.status = ModelStatus::DownloadFailed;
                        store_model_entries!(ui).set_row_data(index, entry.clone());
                        update_db_entry(&ui, entry.into());
                        return;
                    }
                });
            }
        }
    });
}

fn import_model(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    tokio::spawn(async move {
        let Some(file_path) = picker_file(ui_weak.clone(), &tr("Choose a model file")) else {
            return;
        };

        let id = Uuid::new_v4().to_string();
        let model_name = cutil::fs::file_name(&file_path);
        let file_size = cutil::fs::file_size(&file_path.as_path());
        let file_size = cutil::str::pretty_size_string(file_size);

        _ = slint::invoke_from_event_loop(move || {
            let ui = ui_weak.unwrap();
            let entry = UIModelEntry {
                id: id.into(),
                name: model_name.into(),
                file_path: file_path.to_string_lossy().to_string().into(),
                file_size: file_size.into(),
                source: ModelSource::Local,
                status: ModelStatus::Import,
                progress: 0.0,
            };

            store_model_entries!(ui).push(entry.clone());
            add_db_entry(&ui, entry.into());
        });
    });
}

fn manual_download_model(ui: &AppWindow) {
    global_util!(ui).invoke_open_url(
        "Default".to_string().into(),
        whisper_model_downloader::WHISPER_MODELS_WEBSITE
            .to_string()
            .into(),
    );
}

fn cancel_download_model(ui: &AppWindow, index: i32) {
    let id = store_model_entries!(ui)
        .row_data(index as usize)
        .unwrap()
        .id
        .clone();

    if let Some(sig) = get_cancel_sig(&id) {
        sig.store(true, Ordering::Relaxed);
    }
}

fn remove_model(ui: &AppWindow, index: i32) {
    let entry = store_model_entries!(ui).remove(index as usize);
    toast_success!(ui, tr("remove model successfully"));

    delete_db_entry(ui, entry.id.into());
}

pub fn get_model_path(ui: &AppWindow, model_name: &str) -> Option<String> {
    match store_model_entries!(ui)
        .iter()
        .find(|item| item.name == model_name)
    {
        Some(entry) => Some(entry.file_path.to_string()),
        _ => None,
    }
}

fn add_db_entry(ui: &AppWindow, entry: ModelEntry) {
    let ui = ui.as_weak();
    tokio::spawn(async move {
        let data = serde_json::to_string(&entry).unwrap();
        match db::entry::insert(DB_TABLE, &entry.id, &data).await {
            Err(e) => toast::async_toast_warn(
                ui,
                format!("{}. {}: {e}", tr("insert entry failed"), tr("Reason")),
            ),
            _ => (),
        }
    });
}

fn update_db_entry(ui: &AppWindow, entry: ModelEntry) {
    let ui = ui.as_weak();
    tokio::spawn(async move {
        let data = serde_json::to_string(&entry).unwrap();
        match db::entry::update(DB_TABLE, &entry.id, &data).await {
            Err(e) => toast::async_toast_warn(
                ui,
                format!("{}. {}: {e}", tr("Update entry failed"), tr("Reason")),
            ),
            _ => (),
        }
    });
}

fn delete_db_entry(ui: &AppWindow, id: String) {
    let ui = ui.as_weak();
    tokio::spawn(async move {
        match db::entry::delete(DB_TABLE, &id).await {
            Err(e) => toast::async_toast_warn(
                ui,
                format!("{}. {}: {e:?}", tr("Remove entry failed"), tr("Reason")),
            ),
            _ => (),
        }
    });
}

fn get_cancel_sig(id: &str) -> Option<Arc<AtomicBool>> {
    let sigs = CANCEL_SIGS.lock().unwrap();
    sigs.get(id).cloned()
}

fn add_cancel_sig(id: String) -> Arc<AtomicBool> {
    let sig = Arc::new(AtomicBool::new(false));
    let mut sigs = CANCEL_SIGS.lock().unwrap();
    sigs.insert(id, sig.clone());
    sig
}

impl From<whisper_model_downloader::DownloadStatus> for ModelStatus {
    fn from(status: whisper_model_downloader::DownloadStatus) -> Self {
        match status {
            whisper_model_downloader::DownloadStatus::Finsished => ModelStatus::DownloadFinished,
            whisper_model_downloader::DownloadStatus::Cancelled => ModelStatus::DownloadCancelled,
            whisper_model_downloader::DownloadStatus::Partial => ModelStatus::DownloadFailed,
        }
    }
}
