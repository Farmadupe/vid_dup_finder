use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
};

use gdk_pixbuf::Pixbuf;
use glib::clone;
use gtk::{prelude::*, Button};

use super::{
    gui_thumbnail_set::{GuiThumbnailSet, ThumbChoice},
    gui_zoom::{ZoomState, ZoomValue},
};
use crate::*;

pub struct GuiEntryState {
    thumbs: GuiThumbnailSet,

    thumbs_pixbuf: Option<HashMap<PathBuf, Pixbuf>>,

    thunk: ResolutionThunk,
    single_mode: bool,
    entry_idx: usize,

    excludes: HashSet<PathBuf>,
}

impl GuiEntryState {
    pub fn new(thunk: ResolutionThunk, single_mode: bool, thumb_choice: ThumbChoice, zoom: ZoomState) -> Self {
        let info = thunk
            .entries()
            .into_iter()
            .map(|src_path| (src_path, thunk.hash(src_path)))
            .collect::<Vec<_>>();

        let thumbs = GuiThumbnailSet::new(info, zoom, thumb_choice);

        let mut ret = Self {
            thumbs,
            thumbs_pixbuf: None,
            thunk,
            single_mode,
            entry_idx: 0,
            excludes: Default::default(),
        };

        ret.regen_thumbs_pixbuf();

        //trace!("entry creation: single={}", ret.single_mode);

        ret
    }

    pub fn increment(&mut self) {
        if self.entry_idx < self.thunk.len() - 1 {
            self.entry_idx += 1;
        } else {
            self.entry_idx = 0;
        }

        let name_of_next = *self.thunk.entries().get(self.entry_idx).unwrap();
        if self.excludes.contains(name_of_next) {
            self.increment();
        }
    }

    pub fn decrement(&mut self) {
        if self.entry_idx > 0 {
            self.entry_idx -= 1;
        } else {
            self.entry_idx = self.thunk.len() - 1;
        }

        let name_of_next = *self.thunk.entries().get(self.entry_idx).unwrap();
        if self.excludes.contains(name_of_next) {
            self.decrement();
        }
    }

    pub fn set_single_mode(&mut self, val: bool) {
        self.single_mode = val;
        self.entry_idx = 0;
    }

    pub fn render_current_entry(&self) -> gtk::Box {
        self.render_entry(self.entry_idx)
    }

    pub fn render(&self) -> gtk::Box {
        if self.single_mode {
            self.render_current_entry()
        } else {
            self.render_whole_thunk()
        }
    }

    pub fn render_whole_thunk(&self) -> gtk::Box {
        let entry_box = gtk::Box::new(gtk::Orientation::Vertical, 25);

        for (i, filename) in self.thunk.entries().iter().enumerate() {
            if !self.excludes.contains(*filename) {
                let row = self.render_entry(i);
                entry_box.add(&row);
            }
        }

        entry_box
    }

    pub fn distance(&self) -> String {
        match self.thunk.distance() {
            // Format the normalized distance as a percentage
            Some(distance) => {
                let similarity = ((1.0 - distance) * 100.0) as u32;
                format!("Similarity: {}%", similarity)
            }
            None => "?????".to_string(),
        }
    }

    fn render_entry(&self, i: usize) -> gtk::Box {
        let entry_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let text_stack = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text_stack.set_size_request(300, -1);

        let entries = self.thunk.entries();
        let src_path = *entries.get(i).unwrap();

        let i_label = gtk::Label::new(Some(&i.to_string()));
        i_label.set_width_chars(2);
        i_label.set_halign(gtk::Align::Start);

        let winning_stats = self.thunk.calc_winning_stats(src_path);

        let ref_label = gtk::Label::new(Some(if winning_stats.is_reference { "REF" } else { "   " }));
        ref_label.set_width_chars(3);

        let pngsize_label = gtk::Label::new(Some(if winning_stats.pngsize { "PNG" } else { "   " }));
        pngsize_label.set_width_chars(3);

        let filesize_label = gtk::Label::new(Some(if winning_stats.filesize { "FIL" } else { "   " }));
        filesize_label.set_width_chars(3);

        let res_label = gtk::Label::new(Some(if winning_stats.res { "RES" } else { "   " }));
        res_label.set_width_chars(3);

        let bitrate_label = gtk::Label::new(Some(if winning_stats.bitrate { "BIT" } else { "   " }));
        bitrate_label.set_width_chars(3);

        let audio_label = gtk::Label::new(Some(if winning_stats.has_audio { "AUD" } else { "   " }));

        let duration = self.thunk.render_duration(src_path);
        let duration_label = gtk::Label::new(Some(&duration));
        duration_label.set_halign(gtk::Align::Start);

        let details_1 = self.thunk.render_details_top(src_path);
        let details_label_1 = gtk::Label::new(Some(&details_1));
        details_label_1.set_halign(gtk::Align::Start);

        let details_2 = self.thunk.render_details_bottom(src_path);
        let details_label_2 = gtk::Label::new(Some(&details_2));
        details_label_2.set_halign(gtk::Align::Start);

        let win_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        win_row.add(&ref_label);
        win_row.add(&pngsize_label);
        win_row.add(&filesize_label);
        win_row.add(&res_label);
        win_row.add(&bitrate_label);
        win_row.add(&audio_label);
        text_stack.add(&i_label);
        text_stack.add(&win_row);
        text_stack.add(&duration_label);
        text_stack.add(&details_label_1);
        text_stack.add(&details_label_2);

        let button = Button::with_label(&src_path.to_string_lossy());
        button.set_halign(gtk::Align::Start);
        let src_path = src_path.to_path_buf();
        button.connect_clicked(clone!(@strong src_path => move |_|Self::vlc_video_inner(&src_path)));

        let thumb = self.thumbs_pixbuf.as_ref().unwrap().get(&src_path).unwrap();

        let image = gtk::Image::from_pixbuf(Some(thumb));
        image.set_halign(gtk::Align::Start);

        let text_then_image = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        text_then_image.add(&text_stack);
        text_then_image.add(&image);

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);

        entry_box.add(&separator);
        entry_box.add(&button);
        entry_box.add(&text_then_image);

        entry_box
    }

    pub fn set_zoom(&mut self, val: ZoomState) {
        self.thumbs.set_zoom(val);
        self.regen_thumbs_pixbuf();
    }

    pub fn set_choice(&mut self, val: ThumbChoice) {
        self.thumbs.set_choice(val);
        self.regen_thumbs_pixbuf();
    }

    pub fn vlc_video(&self, idx: usize) {
        if let Some(filename) = self.thunk.entries().get(idx) {
            Self::vlc_video_inner(filename);
        }
    }

    pub fn vlc_current_video(&self) {
        if self.single_mode {
            self.vlc_video(self.entry_idx);
        }
    }

    pub fn nautilus_file(&self, idx: usize) {
        if let Some(filename) = self.thunk.entries().get(idx) {
            Self::nautilus_file_inner(filename);
        }
    }

    pub fn nautilus_current_file(&self) {
        if self.single_mode {
            self.nautilus_file(self.entry_idx);
        }
    }

    pub fn exclude(&mut self, idx: usize) {
        if let Some(filename) = self.thunk.entries().get(idx) {
            if self.excludes.len() < self.thunk.entries().len() - 1 {
                self.excludes.insert(filename.to_path_buf());
            }
        }

        if idx == self.entry_idx {
            self.increment();
        }
    }

    pub fn include(&mut self, idx: usize) {
        trace!("Including video {}", idx);
        if let Some(filename) = self.thunk.entries().get(idx) {
            self.excludes.remove(*filename);
        }
    }

    pub fn resolve(&mut self, resolution: &str) {
        trace!("Resolving! with {}", resolution);
        if let Err(e) = self.thunk.resolve(resolution) {
            warn!("{}", e.to_string());
        }
    }

    pub fn vlc_all_slave(&self) {
        let mut path_iter = self.thunk.entries().into_iter();

        //let first_arg = shell_words::quote(&path_iter.next().unwrap()).to_string();
        let main_vid = path_iter.next().unwrap();
        let follow_vid = path_iter.next().unwrap();

        let mut follow_arg = OsString::from("--input_slave=");
        follow_arg.push(follow_vid);
        let mut command = std::process::Command::new("vlc");
        let command = command.arg(main_vid).arg(&follow_arg);

        if let Err(e) = command.spawn() {
            warn!("Failed to start vlc at {}: {}", follow_arg.to_string_lossy(), e);
        }
    }

    pub fn vlc_all_seq(&self) {
        let mut command = std::process::Command::new("vlc");
        for entry in self.thunk.entries() {
            command.arg(entry);
        }

        if let Err(e) = command.spawn() {
            warn!("Failed to start vlc: {}", e);
        }
    }

    fn nautilus_file_inner(path: &Path) {
        if let Err(e) = std::process::Command::new("nautilus").arg(path).spawn() {
            warn!("Failed to start nautilus at {}: {}", path.display(), e);
        }
    }

    fn vlc_video_inner(path: &Path) {
        if let Err(e) = std::process::Command::new("vlc").arg(path).spawn() {
            warn!("Failed to start vlc at {}: {}", path.display(), e);
        }
    }

    fn regen_thumbs_pixbuf(&mut self) {
        self.thumbs_pixbuf = Some(self.thumbs.get_pixbufs())
    }
}

#[derive(Debug, PartialEq)]
enum KeypressState {
    None,
    Exclude,
    Include,
    View,
    JumpTo,
    Resolve,
    Nautilus,
}

pub struct GuiState {
    thunks: Vec<ResolutionThunk>,
    single_mode: bool,
    zoom: ZoomState,
    thumb_choice: ThumbChoice,
    thunk_idx: usize,
    current_thunk: GuiEntryState,
    keypress_state: KeypressState,
    keypress_string: String,
}

impl GuiState {
    pub fn new(thunks: Vec<ResolutionThunk>, single_mode: bool) -> Self {
        let default_zoom_state = ZoomState::new(50, 1000, 50, 50);

        let current_entry = GuiEntryState::new(
            thunks.get(0).unwrap().clone(),
            single_mode,
            ThumbChoice::Video,
            default_zoom_state,
        );

        Self {
            thunks,
            single_mode,
            zoom: default_zoom_state,
            thunk_idx: 0,
            current_thunk: current_entry,

            thumb_choice: ThumbChoice::Video,

            keypress_state: KeypressState::None,
            keypress_string: "".to_string(),
        }
    }

    pub fn next_thunk(&mut self) {
        if self.thunk_idx < self.thunks.len() - 1 {
            self.thunk_idx += 1;
        } else {
            self.thunk_idx = 0;
        }

        self.gen_thunk();
    }

    pub fn prev_thunk(&mut self) {
        if self.thunk_idx > 0 {
            self.thunk_idx -= 1;
        } else {
            self.thunk_idx = self.thunks.len() - 1;
        }

        self.gen_thunk();
    }

    pub fn render(&self) -> gtk::Box {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);

        let label_text = format!("{:?} {}", self.keypress_state, self.keypress_string);

        let the_label = gtk::Label::new(Some(&label_text));
        the_label.set_halign(gtk::Align::Start);
        b.add(&the_label);

        let entries = self.current_thunk.render();
        b.add(&entries);

        b
    }

    pub fn increment_thunk_entry(&mut self) {
        self.current_thunk.increment();
    }

    pub fn decrement_thunk_entry(&mut self) {
        self.current_thunk.decrement();
    }

    pub fn set_single_mode(&mut self, val: bool) {
        self.single_mode = val;
        self.current_thunk.set_single_mode(self.single_mode)
    }

    pub fn get_single_mode(&self) -> bool {
        self.single_mode
    }

    pub fn zoom_in(&mut self) {
        self.zoom = self.zoom.zoom_in();
        self.current_thunk.set_zoom(self.zoom)
    }

    pub fn zoom_out(&mut self) {
        self.zoom = self.zoom.zoom_out();
        self.current_thunk.set_zoom(self.zoom)
    }

    pub fn set_native(&mut self, val: bool) {
        self.zoom = self.zoom.set_native(val);
        self.current_thunk.set_zoom(self.zoom)
    }

    pub fn get_native(&self) -> bool {
        self.zoom.get() == ZoomValue::Native
    }

    pub fn set_view_spatial(&mut self, val: bool) {
        if val {
            self.thumb_choice = ThumbChoice::Spatial;
        } else {
            self.thumb_choice = ThumbChoice::Video;
        }

        self.current_thunk.set_choice(self.thumb_choice);
    }

    pub fn set_view_temporal(&mut self, val: bool) {
        if val {
            self.thumb_choice = ThumbChoice::Temporal;
        } else {
            self.thumb_choice = ThumbChoice::Video;
        }
        self.current_thunk.set_choice(self.thumb_choice);
    }

    pub fn set_view_rebuilt(&mut self, val: bool) {
        if val {
            self.thumb_choice = ThumbChoice::Rebuilt;
        } else {
            self.thumb_choice = ThumbChoice::Video;
        }
        self.current_thunk.set_choice(self.thumb_choice);
    }

    pub fn set_cropdetect(&mut self, val: bool) {
        if val {
            self.thumb_choice = ThumbChoice::CropdetectVideo;
        } else {
            self.thumb_choice = ThumbChoice::Video;
        }
        self.current_thunk.set_choice(self.thumb_choice);
    }

    pub fn press_key(&mut self, key: &str) {
        match key {
            "a" => {
                self.keypress_string.push('a');
            }

            "i" => {
                self.keypress_state = KeypressState::Include;
                self.keypress_string.clear();
            }

            "j" => {
                self.keypress_state = KeypressState::JumpTo;
                self.keypress_string.clear();
            }

            "k" => {
                self.keypress_state = KeypressState::Resolve;
                self.keypress_string.clear();
            }

            "b" => {
                self.current_thunk.vlc_all_slave();
            }

            "m" => {
                self.current_thunk.vlc_all_seq();
            }

            "n" => {
                self.keypress_state = KeypressState::Nautilus;
                self.keypress_string.clear();
            }

            "s" => {
                self.keypress_string.push('s');
            }

            "t" => {
                self.keypress_string.push('t');
            }

            "v" => {
                self.keypress_state = KeypressState::View;
                self.keypress_string.clear();
            }

            "x" => {
                self.keypress_state = KeypressState::Exclude;
                self.keypress_string.clear();
            }

            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "kp_0" | "kp_1" | "kp_2" | "kp_3" | "kp_4"
            | "kp_5" | "kp_6" | "kp_7" | "kp_8" | "kp_9" => {
                self.keypress_string.push(key.chars().last().unwrap());
            }

            "space" => {
                self.keypress_string.push(' ');
            }

            "backspace" => {
                self.keypress_string.pop();
            }

            "return" | "kp_enter" => {
                if let Ok(idx) = self.keypress_string.parse::<usize>() {
                    match self.keypress_state {
                        KeypressState::None => {}
                        KeypressState::Exclude => self.current_thunk.exclude(idx),
                        KeypressState::Include => self.current_thunk.include(idx),
                        KeypressState::View => self.current_thunk.vlc_video(idx),
                        KeypressState::JumpTo => {
                            if idx < self.thunks.len() {
                                self.thunk_idx = idx;
                                self.gen_thunk();
                            }
                        }
                        KeypressState::Resolve => {
                            self.current_thunk.resolve(&self.keypress_string);
                            self.next_thunk()
                        }
                        KeypressState::Nautilus => self.current_thunk.nautilus_file(idx),
                    }
                } else {
                    match self.keypress_state {
                        KeypressState::None => {}
                        KeypressState::Exclude => {}
                        KeypressState::Include => {}
                        KeypressState::View => self.current_thunk.vlc_current_video(),
                        KeypressState::JumpTo => {}
                        KeypressState::Resolve => {
                            self.current_thunk.resolve(&self.keypress_string);
                            self.next_thunk()
                        }
                        KeypressState::Nautilus => self.current_thunk.nautilus_current_file(),
                    }
                }

                self.keypress_state = KeypressState::None;
                self.keypress_string.clear();
            }
            _ => {
                //debug!("state: Unhandled keypress: {}", key);
            }
        }
    }

    pub fn current_idx(&self) -> usize {
        self.thunk_idx
    }

    pub fn idx_len(&self) -> usize {
        self.thunks.len()
    }

    pub fn current_distance(&self) -> String {
        self.current_thunk.distance()
    }

    fn gen_thunk(&mut self) {
        //trace!("Moving to thunk {}", self.thunk_idx);
        self.current_thunk = GuiEntryState::new(
            self.thunks.get(self.thunk_idx).unwrap().clone(),
            self.single_mode,
            self.thumb_choice,
            self.zoom,
        );
    }
}
