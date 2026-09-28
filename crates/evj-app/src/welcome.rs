//! EVJ start screen: start actions on the left, recent shows as cards on the right, Resume at the bottom.
use crate::thumbs::{Thumb, Thumbnailer};
use crate::ui::{Actions, Menu, UiState};
use egui::{Color32, RichText, vec2};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RecentCard {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    pub saved: Option<String>,
    pub first_clip: Option<PathBuf>,
    pub exists: bool,
}

fn saved_label(meta: &std::fs::Metadata) -> Option<String> {
    let age = meta.modified().ok()?.elapsed().ok()?.as_secs();
    Some(match age {
        0..=3599 => format!("saved {} min ago", age / 60),
        3600..=86_399 => format!("saved {} h ago", age / 3600),
        _ => format!("saved {} days ago", age / 86_400),
    })
}

/// Cards for the recent-shows list (reads each show file once, when the welcome opens).
pub fn cards(recent: &[PathBuf]) -> Vec<RecentCard> {
    recent
        .iter()
        .map(|p| {
            let meta = std::fs::metadata(p).ok();
            let first_clip = meta
                .as_ref()
                .and_then(|_| evj_core::io::load(p).ok())
                .and_then(|proj| {
                    let clips: Vec<PathBuf> = proj.decks.iter().flat_map(|d| d.slots.iter().flatten().flatten()).map(|c| c.path.clone()).collect();
                    // A picture beats a sound file for the card.
                    clips.iter().find(|p| !is_sound(p)).or(clips.first()).cloned()
                });
            RecentCard {
                path: p.clone(),
                name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                folder: p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
                saved: meta.as_ref().and_then(saved_label),
                exists: meta.is_some(),
                first_clip,
            }
        })
        .collect()
}

fn is_sound(p: &std::path::Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    ["mp3", "wav", "m4a", "aac", "flac", "wma"].contains(&ext.as_str())
}

fn action(ui: &mut egui::Ui, title: &str, sub: &str) -> bool {
    let r = ui.add(egui::Button::new(RichText::new(title).size(16.0).strong()).min_size(vec2(260.0, 40.0)));
    ui.label(RichText::new(sub).small().weak());
    ui.add_space(6.0);
    r.clicked()
}

pub fn screen(ui: &mut egui::Ui, st: &mut UiState, thumbs: &mut Thumbnailer, act: &mut Actions) {
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_width(300.0);
            let has_show = st.project_path.is_some() || st.project.decks.iter().any(|d| d.slots.iter().flatten().any(Option::is_some));
            if has_show && ui.button("← Back to the show").clicked() {
                st.show_welcome = false;
            }
            if let Some(logo) = st.logo_texture() {
                ui.image((logo, vec2(160.0, 103.0)));
            }
            ui.label(RichText::new("Event Visual Jockey").size(24.0).strong());
            ui.label(RichText::new("Visuals, presentations and sound for live events").weak());
            ui.add_space(20.0);
            if action(ui, "New show", "Empty grid, 4 layers") {
                act.menu = Some(Menu::New);
                st.show_welcome = false;
            }
            if action(ui, "Open show…", "A .vjproj file (Ctrl+O)") {
                act.menu = Some(Menu::Open);
                st.show_welcome = false;
            }
            if action(ui, "Import media folder…", "Every video, image and sound, in name order") {
                act.menu = Some(Menu::ImportFolder);
                st.show_welcome = false;
            }
            if action(ui, "Import presentation…", "PowerPoint or PDF into the grid") {
                act.menu = Some(Menu::ImportPresentation);
                st.show_welcome = false;
            }
        });
        ui.add_space(32.0);
        ui.vertical(|ui| {
            ui.label(RichText::new("Recent shows").size(18.0).strong());
            ui.add_space(8.0);
            if st.recent_cards.is_empty() {
                ui.label(RichText::new("Shows you open or save appear here.").weak());
            }
            let cards = st.recent_cards.clone();
            let height = (ui.available_height() - 60.0).max(120.0);
            egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
                egui::Grid::new("recent_cards").spacing(vec2(12.0, 12.0)).show(ui, |ui| {
                    for (i, c) in cards.iter().enumerate() {
                        let frame = egui::Frame::new().fill(Color32::from_gray(32)).corner_radius(6.0).inner_margin(8.0);
                        let inner = frame.show(ui, |ui| ui.vertical(|ui| {
                            ui.set_width(220.0);
                            let tex = c.first_clip.as_ref().and_then(|p| match thumbs.get(p) {
                                Thumb::Ready { tex, .. } => Some(tex.id()),
                                _ => None,
                            });
                            match tex {
                                Some(id) => {
                                    ui.image((id, vec2(220.0, 124.0)));
                                }
                                None => {
                                    let (r, _) = ui.allocate_exact_size(vec2(220.0, 124.0), egui::Sense::hover());
                                    ui.painter().rect_filled(r, 4.0, Color32::from_gray(20));
                                }
                            }
                            ui.label(RichText::new(&c.name).strong());
                            ui.label(RichText::new(&c.folder).small().weak());
                            let (info, color) = if c.exists {
                                (c.saved.clone().unwrap_or_default(), Color32::from_gray(150))
                            } else {
                                ("not found".to_string(), Color32::from_rgb(235, 60, 60))
                            };
                            ui.label(RichText::new(info).small().color(color));
                        }));
                        let r = inner.response.interact(egui::Sense::click());
                        if c.exists && r.on_hover_text(c.path.to_string_lossy()).clicked() {
                            act.menu = Some(Menu::OpenRecent(c.path.clone()));
                            st.show_welcome = false;
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            });
            if let Some(last) = cards.first().filter(|c| c.exists) {
                ui.add_space(8.0);
                let b = egui::Button::new(RichText::new(format!("Resume last show: {}", last.name)).strong().color(Color32::WHITE)).fill(Color32::from_rgb(40, 150, 230));
                if ui.add(b).clicked() {
                    act.menu = Some(Menu::OpenRecent(last.path.clone()));
                    st.show_welcome = false;
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_read_the_first_clip_and_flag_missing_shows() {
        let dir = std::env::temp_dir().join(format!("evj-welcome-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let show = dir.join("Gala.vjproj");
        let mut p = evj_core::model::Project::new_default();
        p.decks[0].place(1, 2, &[dir.join("intro.mp4")]);
        evj_core::io::save(&p, &show).unwrap();
        let c = cards(&[show.clone(), dir.join("gone.vjproj")]);
        assert_eq!(c[0].name, "Gala");
        assert!(c[0].exists && c[0].saved.is_some());
        assert_eq!(c[0].first_clip, Some(dir.join("intro.mp4")));
        assert!(!c[1].exists && c[1].first_clip.is_none());
    }
}
