// Coppied from cosmic-app-list
// - Put in a library? libcosmic?

use cosmic::desktop::fde;
use itertools::Itertools;
use std::path::PathBuf;

pub async fn icon_for_app_id(app_id: String) -> Option<PathBuf> {
    tokio::task::spawn_blocking(|| {
        Some(
            desktop_info_for_app_ids(vec![app_id])
                .into_iter()
                .next()?
                .icon,
        )
    })
    .await
    .unwrap()
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
struct DesktopInfo {
    id: String,
    wm_class: Option<String>,
    icon: PathBuf,
    exec: String,
    name: String,
    path: PathBuf,
}

fn default_app_icon() -> PathBuf {
    freedesktop_icons::lookup("application-default")
        .with_theme("Cosmic")
        .force_svg()
        .with_cache()
        .find()
        .or_else(|| {
            freedesktop_icons::lookup("application-x-executable")
                .with_theme("default")
                .with_size(128)
                .with_cache()
                .find()
        })
        .unwrap_or_default()
}

fn desktop_info_for_app_ids(mut app_ids: Vec<String>) -> Vec<DesktopInfo> {
    let app_ids_clone = app_ids.clone();
    let mut ret = fde::Iter::new(fde::default_paths())
        .filter_map(|path| {
            fde::DesktopEntry::from_path::<String>(path.clone(), None)
                .ok()
                .and_then(|de| {
                    if let Some(i) = app_ids.iter().position(|s| {
                        *s == de.appid || s.eq(&de.startup_wm_class().unwrap_or_default())
                    }) {
                        let icon = freedesktop_icons::lookup(de.icon().unwrap_or(&de.appid))
                            .with_size(128)
                            .with_cache()
                            .find()
                            .unwrap_or_else(default_app_icon);
                        app_ids.remove(i);

                        Some(DesktopInfo {
                            id: de.appid.to_string(),
                            wm_class: de.startup_wm_class().map(ToString::to_string),
                            icon,
                            exec: de.exec().unwrap_or_default().to_string(),
                            name: de.name::<String>(&[]).unwrap_or_default().to_string(),
                            path: path.clone(),
                        })
                    } else {
                        None
                    }
                })
        })
        .collect_vec();
    ret.append(
        &mut app_ids
            .into_iter()
            .map(|id| DesktopInfo {
                id,
                icon: default_app_icon(),
                ..Default::default()
            })
            .collect_vec(),
    );
    ret.sort_by(|a, b| {
        app_ids_clone
            .iter()
            .position(|id| id == &a.id || Some(id) == a.wm_class.as_ref())
            .cmp(
                &app_ids_clone
                    .iter()
                    .position(|id| id == &b.id || Some(id) == b.wm_class.as_ref()),
            )
    });
    ret
}
