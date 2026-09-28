# EVJ — Status PRD v2

Status per requirement beserta buktinya: nama test otomatis (`cargo test`, 160+ test; test GPU memakai WARP), hasil bench di laptop batas bawah (Ryzen 7 5700U + Vega 8, 8 GB), atau item di `docs/TESTING-CHECKLIST.md` untuk pengujian manual.

Legenda: ✅ selesai & terbukti · 🟡 selesai, perlu uji manual / laptop RTX · ⚠️ sebagian (lihat catatan)

## Goals

| ID | Status | Bukti / catatan |
|---|---|---|
| G1 Show 4 jam tanpa crash, memori datar | 🟡 | Mode `--soak HOURS`. Smoke 6 + 5 menit: 0 crash, 0 error layer, memori naik-turun 400–1.060 MB mengikuti clip 4K, tanpa tren naik. **Run 4 jam dilakukan user** (checklist G1). |
| G2 4 HAP 1080p + 2 efek/layer + 2 output, 60 fps | ⚠️ | Lihat bagian *Bench G2* di bawah. RTX: checklist G2. |
| G3 0 drop saat trigger / transisi / slide / UI sibuk | ⚠️ | Tanpa efek berat: drop jarang (M1: 7 drop / 20 dtk untuk 4 HAP). Soak acak (termasuk video 4K H.264) ± 5–15 drop/menit di Vega 8. UI dan decoder tidak memblokir render thread (arsitektur Command/Snapshot, trigger async). |
| G4 Workflow tanpa Resolume / PowerPoint / audio player | ✅ | Grid, layer, efek, BPM, transisi, multi-output, slide PPT/PDF (PowerPoint hanya dipakai tersembunyi saat import), audio; diverifikasi dengan materi acara Juwangi. |
| G5 Installer + checklist | ✅ | `dist/EVJ-Setup-0.1.0.exe` (Inno Setup, CRT statis) + `docs/TESTING-CHECKLIST.md`. |

## Fitur

| ID | Status | Bukti |
|---|---|---|
| F-GRID-1 | ✅ | `place_fills_to_the_right_and_grows_columns`, drag-drop Explorer, thumbnail async (`thumbs.rs`) |
| F-GRID-2 | ✅ | `keymap_is_saved_with_the_project`, `bind_resolve_and_rebind`, trigger kolom / clear di UI |
| F-GRID-3 | ✅ | `new_deck_matches_layer_count`, tab deck |
| F-GRID-4 | ✅ | `ping_pong_reflects_and_flips_direction`, `negative_speed_runs_backwards_and_wraps`, `loop_wraps_inside_in_out`, `bpm_sync_speed` |
| F-GRID-5 (S) | ⚠️ | Clip baru di-decode di latar dan baru ditukar setelah frame pertama siap di GPU (tanpa layar hitam). Pre-warm *sebelum* trigger belum ada. |
| F-LAYER-1 | ✅ | `bypass_and_solo`, `opacity_mixes_layers_and_clear_removes`, efek & transisi per layer |
| F-LAYER-2 | ✅ | `all_blend_modes_match_cpu_reference`, `fixed_blends_match_the_shader_blend` |
| F-KEY-1 | ✅ | `keymap_is_saved_with_the_project` |
| F-MEDIA-1 | ✅ | `hap1_decodes_to_bc1_of_expected_size`, `hap_alpha_and_q_formats`, `ycocg_block_converts_to_rgb`, `hap_is_random_access_and_serves_requests` |
| F-MEDIA-2 | ✅ | `decodes_h264_with_hardware_device`, `hardware_frames_are_nv12_with_colour_matrix`, `decodes_h264_to_system_memory_and_rewinds` |
| F-MEDIA-3 | ✅ | `prores_falls_back_to_ffmpeg`, `prores_thumbnail_comes_from_ffmpeg`, badge **HEAVY** di grid |
| F-MEDIA-4 | ✅ | `loads_png_as_bgra`, `loads_jpg`, `exif_rotation_is_applied`, `huge_images_are_downscaled` |
| F-MEDIA-5 | ✅ | `converts_h264_to_hap_with_progress`, `odd_sizes_are_padded_and_hap_q_works`, `bad_input_reports_ffmpeg_error_and_cancel_stops` |
| F-MEDIA-6 | ✅ | `missing_file_reports_error_and_engine_keeps_working`, `truncated_and_garbage_are_errors`, `bad_inputs_are_errors_not_panics` |
| F-FX-1 | ✅ | 16 efek bawaan (`every_builtin_compiles`, `some_effects_change_the_image_by_default`) |
| F-FX-2 | ✅ | `layer_clip_and_composition_effects`, editor rantai (urut, bypass) |
| F-FX-3 | ✅ | `lfo_waves`, `lfo_maps_into_min_max`, `resolve_defaults_overrides_and_lfo` |
| F-FX-4 | ✅ | `custom_folder_hot_reload_and_errors`, `effects/README.md` |
| F-BPM-1 | ✅ | `tap_tempo_averages_recent_taps`, `bpm_is_clamped_and_resync_nudge`, indikator beat |
| F-TR-1 | ✅ | 12 transisi, `transitions_start_at_source_and_end_at_destination`, `transitions_mix_in_premultiplied_alpha` |
| F-TR-2 | ✅ | `transition_preview_renders_while_selected`, Transition Manager |
| F-TR-3 | ✅ | `time_in_seconds_or_beats`, `easing_curves` |
| F-TR-4 | ✅ | `preset_resolution_order`, transisi antar slide (`slides_step_through_images_and_click_animations`) |
| F-TR-5 | ✅ | `custom_transition_is_imported_from_the_folder` |
| F-OUT-1 | ✅ | Output Manager + monitor fullscreen / window (checklist D1) |
| F-OUT-2 | ✅ | `output_can_show_a_single_layer_or_a_test_pattern`; speaker output = sumber *Layer* = layer presentasi |
| F-OUT-3 | ✅ | `output_slices_crop_the_composition`, `slice_rects_are_clamped_and_serialized` |
| F-OUT-4 | ✅ | Identify, test pattern, preview output di UI (checklist D2) |
| F-OUT-5 | 🟡 | Poll monitor tiap 1 dtk, output dibuka/tutup ulang (checklist D5) |
| F-PRES-1 | ✅ | `pdf_pages_become_slides` |
| F-PRES-2 | ✅ | `pptx_slides_notes_and_click_animations`; PPTX acara (36 MB) ter-import dalam ±10–15 dtk |
| F-PRES-3 | 🟡 | `without_powerpoint_the_fallback_or_a_clear_message` (LibreOffice tidak terpasang di laptop ini: checklist F4) |
| F-PRES-4 | ✅ | `next_walks_click_steps_then_slides`, `prev_goes_back_one_step_or_to_the_end_of_the_previous_slide`, `slides_step_through_images_and_click_animations` |
| F-PRES-5 | ✅ | Materi checklist (`materi_decks_are_saved_relative_and_relinked`), Present / End menandai ✓ |
| F-PRES-6 | ✅ | Raw Input: `clicker_keys`; diuji dengan PageDown tersuntik saat EVJ aktif **dan** di belakang terminal (4 → 5), panah diabaikan saat tidak fokus. Clicker fisik: checklist F5 |
| F-PRES-7 | ✅ | `pointer_dot_and_spotlight_on_a_layer`, `pointer_follows_mouse_only_when_on`, tombol UI + hotkey L |
| F-PRES-8 | ✅ | Presenter view (output sekarang, slide berikut, catatan, timer, jam) — diverifikasi dengan screenshot |
| F-AUD-1 | ✅ | `video_clock_follows_clip_audio`, `clips_report_their_audio` |
| F-AUD-2 | ✅ | `audio_only_clip_plays_without_picture`, `audio_files_open_as_audio_only_clips` |
| F-AUD-3 | ✅ | `mixes_voices_with_gain_and_limits`, `master_gain_and_panic_fade`, `meters_report_peaks` |
| F-AUD-4 | ✅ | `plays_through_the_default_device`; fallback saat headset dicolok/dicabut terjadi live saat bench (log: *output device lost, switched to Headphones*) |
| F-AUD-5 | ✅ | `gain_ramps_linearly_without_jumps`, penghitung underrun (`underruns_count_only_gaps_while_decoding`); 0 underrun di semua bench |
| F-PRJ-1 | ✅ | `save_load_round_trip_with_relative_media`, deck import di folder `decks` project |
| F-PRJ-2 | ✅ | `save_leaves_no_temp_files`, recovery autosave (checklist B10) |
| F-PRJ-3 | ✅ | `missing_and_relink` |
| F-SYS-1 | ✅ | Jendela Perf: fps, grafik frame time, status decoder, VRAM, waktu GPU per frame, underrun audio |
| F-SYS-2 | ✅ | Konfirmasi "Outputs are live" |
| F-DIST-1 | ✅ | `installer/evj.iss`, `tools/build-installer.ps1` |
| F-DIST-2 | ✅ | `--bench SECONDS show.vjproj` → fps, p99, drop, CPU, VRAM, memori, waktu GPU per tahap, underrun |
| F-DIST-3 | ✅ | `docs/TESTING-CHECKLIST.md` |

## Sub-proyek B: timeline per clip (perbaikan poin 2–6, 8, 9)

| Poin | Status | Bukti |
|---|---|---|
| 2 Seek per media | 🟡 | `seek_moves_a_random_access_clip_even_when_paused`, `seek_reopens_a_paused_h264_clip_at_the_new_time`, `seek_on_a_program_layer`, `bar_position_maps_to_seconds`; checklist B21 |
| 3 Audio tempel | 🟡 | `image_with_attached_audio_lasts_as_long_as_the_audio`, `attached_audio_plays_with_the_picture`, `replace_silences_the_clips_own_sound_and_mix_keeps_it`, `attached_audio_is_media`; B24 |
| 4 Gelombang suara | 🟡 | `bins_hold_the_peak_of_each_slice`, `decodes_a_real_file`; B23 |
| 5 Rangkaian + loop A–B | 🟡 | `queued_clip_follows_without_a_gap`, `trigger_drops_the_queue_and_a_missing_queued_file_keeps_the_current_clip`, `sequences_need_two_filled_slots_in_one_layer_and_are_ordered`, `the_run_advances_and_ends_or_loops`, `next_now_triggers_the_following_clip`; B25 |
| 6 Start / end | 🟡 | `h264_in_and_out_points_are_honoured`, `sequential_clip_can_start_at_a_fraction`, `markers_keep_order_and_a_minimum_length`, `i_and_o_set_the_markers_from_the_preview`; B22 |
| 8 Scene | 🟡 | `scene_preview_blends_the_layers`, `a_selected_scene_cues_every_layer_of_its_column`, `scenes_have_default_and_custom_names`; B26 |
| 9 PDF/PPTX di grid | 🟡 | `presentation_files_are_recognised`, `a_finished_import_turns_the_slot_into_the_deck`, `the_topmost_program_deck_owns_the_clicker`, `materi_moves_into_the_presentation_layer`; B27, B28 |

## Sub-proyek C: audio Preview / Program (perbaikan poin 16)

| Poin | Status | Bukti |
|---|---|---|
| 16 Out audio Preview terpisah | 🟡 | `stereo_lands_on_its_pair_and_the_rest_is_silent`, `channel_pairs_follow_the_device`, `a_pair_beyond_the_device_is_an_error`, `preview_route_does_not_fall_back`, `the_cued_clip_sounds_on_preview`, `a_missing_preview_device_reports_and_program_plays_on`, `a_preview_route_on_the_program_pair_is_refused`, `changing_the_program_device_restarts_attached_audio`, `the_same_device_and_pair_clashes`, `settings_round_trip_and_keep_the_shortcuts`; checklist E6–E9 (dua perangkat / soundcard 4 channel) |

## Sub-proyek D: panel modular (perbaikan poin 17)

| Poin | Status | Bukti |
|---|---|---|
| 17 Kotak modular per fitur | 🟡 | `live_preset_shows_the_show_panels`, `setup_preset_adds_the_managers`, `toggle_closes_and_reopens_and_open_is_idempotent`, `layouts_round_trip`, `garbage_or_unknown_layouts_fall_back_to_live`, `grid_and_program_always_come_back`, `saved_next_to_the_other_settings`, `saving_is_throttled_and_only_on_change`, `old_toggles_open_their_tab`, `locked_tabs_are_not_closeable`; checklist D8–D11 |

## Non-fungsional

| ID | Status | Bukti / catatan |
|---|---|---|
| NFR-PERF-1 | ⚠️ | 2 H.264 1080p via hardware decode: 60 fps / 0 drop (M0). G2 di Vega 8: lihat bench. |
| NFR-PERF-2 | ✅ | CPU 7–10 % pada G2, VRAM ±105 MB, memori proses ±180 MB; startup 509 ms (`ready in 509 ms` di log). |
| NFR-PERF-3 | ⚠️ | Trigger & slide async (tidak memblokir render); latensi belum diukur per frame. Slide berikutnya dibuka lebih dulu (preload), jadi klik tidak menunggu decode PNG. |
| NFR-STAB-1 | ✅ | Tidak ada `unwrap` pada input eksternal (hanya konversi slice panjang tetap setelah cek panjang); `panics_leave_a_report`, error decoder per clip |
| NFR-STAB-2 | ✅ | `recovers_from_device_loss` |
| NFR-STAB-3 | ✅ | `panics_leave_a_report`, minidump ke `%APPDATA%\EVJ\crash` |
| NFR-HW-1 | ✅ | Ekspor `NvOptimusEnablement`/`AmdPowerXpressRequestHighPerformance`, `DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE`, peringatan ⚠ bila layar output di GPU lain (`every_display_names_its_gpu`) |
| NFR-HW-2 | ✅ | Flip-model + waitable; jendela yang tertutup / diminimize tidak lagi membuat loop berputar tanpa jeda |
| NFR-HW-3 | ✅ | Tekstur per clip dipakai ulang tiap frame (alokasi hanya saat ukuran berubah); antrian decoder `sync_channel(3)`, cache look-ahead HAP 8 frame (`dropping_player_stops_thread`, `nothing_is_decoded_before_a_request`) |
| NFR-HW-4 | ✅ | `tuning_calls_do_not_fail` (MMCSS, EcoQoS off, timer 1 ms, cegah sleep saat live) |

## Bench G2 (laptop batas bawah)

Skenario: `tools/make-bench-show.py` → 4 HAP 1080p (Hap1, HapQ, Hap Alpha, Hap1), tiap layer Hue Shift + RGB Shift, blend Alpha/Screen, 2 output (satu menampilkan layer 4).

| Tahap | fps | p99 ms | drop / 30 dtk | GPU ms/frame |
|---|---|---|---|---|
| Awal M8 | 47,7 | 37,2 | 648 / 60 dtk | 20,8 |
| + tanpa salin layer untuk clip seukuran composition | 51–54 | 34 | 160–260 | — |
| + blend langsung ke composition (Alpha/Screen/Multiply) | 45–54 (laptop sedang dipakai user saat diukur) | 34–37 | 350–400 | 15,4 (layer 1,4–2,2 ms) |

Run terakhir (laptop diam 2 menit, tepat setelah build installer): 42,2 fps, p99 47,7 ms, 345 drop / 30 dtk, GPU 20,1 ms/frame, CPU 10 %. **G2 belum tercapai di Vega 8.** A/B bergantian (laptop sedang dipakai): 46–57 fps, GPU 15–18 ms/frame; layer yang juga ditampilkan di output lain kini 2 pass, bukan 3.

Catatan: iGPU Vega 8 berbagi daya 15 W dan memori dengan CPU. Angka turun saat laptop sedang dipakai atau baru selesai compile. G2 di laptop RTX: checklist G2.
