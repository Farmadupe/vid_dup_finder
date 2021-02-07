use std::{cell::RefCell, rc::Rc};

use gio::prelude::*;
use glib::clone;
use gtk::{
    prelude::*,
    Application, ApplicationWindow, Box, Button, CheckButton, Label,
    Orientation::{Horizontal, Vertical},
    ToggleButton,
};

use super::gui_state::GuiState;
use crate::library::ResolutionThunk;

pub fn run_gui(thunks: Vec<ResolutionThunk>) {
    if thunks.is_empty() {
        warn!("No matches were found. The GUI will not start");
        return;
    }

    gtk::init().unwrap();

    let state: Rc<RefCell<GuiState>> = Rc::new(RefCell::new(GuiState::new(thunks, false)));

    let application = Application::new(Some("org.gtkrsnotes.demo"), Default::default())
        .expect("failed to initialize GTK application");

    let temp = ();

    application.connect_activate(clone!(
        @strong temp
    => move |app| {
        application_connect_activate_callback(&app, &state)
    }));

    application.run(&[]);
}

fn rerender_gui(state: &Rc<RefCell<GuiState>>, entries_box: &Box, window: &ApplicationWindow, idx_label: &gtk::Label) {
    let state = state.borrow();

    for child in entries_box.get_children() {
        entries_box.remove(&child);
    }

    idx_label.set_text(&format!(
        "duplicate {} / {}. Distance {}",
        state.current_idx() + 1,
        state.idx_len(),
        state.current_distance()
    ));

    let new_interior = state.render();
    entries_box.add(&new_interior);

    window.show_all();
}

//The following callbacks are defined as their own functions because the body of a clone!() macro
//does not get autoindented by rustfmt and does not get autocompleted by rust-analyzer.
//
//SO they are moved outside to restore this functionality.
fn application_connect_activate_callback(app: &Application, state: &Rc<RefCell<GuiState>>) {
    let window = ApplicationWindow::new(app);

    window.set_title("First GTK+ Program");
    window.set_default_size(1000, 800);

    let idx_label = gtk::Label::new(Some(""));

    let scroller = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, gtk::NONE_ADJUSTMENT);

    let nav_and_entries = Box::new(Vertical, 12);

    let entries_box = Box::new(Horizontal, 6);

    let prev_button = Button::with_label("prev");
    prev_button.connect_clicked(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |_| {
        state.borrow_mut().prev_thunk();
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let next_button = Button::with_label("next");

    next_button.connect_clicked(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |_| {
        state.borrow_mut().next_thunk();
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let whole_single_button = ToggleButton::with_label("View single");
    whole_single_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |whole_single_button| {
        let new_single_selected = whole_single_button.get_active();
        state.borrow_mut().set_single_mode(new_single_selected);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));
    let state_mode = state.borrow().get_single_mode();
    whole_single_button.set_active(state_mode);

    let native_res_button = ToggleButton::with_label("View in native res");
    native_res_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |native_res_button| {
        let new_native_res = native_res_button.get_active();
        state.borrow_mut().set_native(new_native_res);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));
    let state_mode = state.borrow().get_native();
    native_res_button.set_active(state_mode);

    let view_spatial_button = CheckButton::with_label("View spatial hash");
    view_spatial_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |view_spatial_button| {


        let new_view_spatial = view_spatial_button.get_active();
        state.borrow_mut().set_view_spatial(new_view_spatial);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let view_temporal_button = CheckButton::with_label("View temporal hash");
    view_temporal_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |view_temporal_button| {
        let new_view_temporal = view_temporal_button.get_active();
        state.borrow_mut().set_view_temporal(new_view_temporal);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let view_rebuilt_button = CheckButton::with_label("View images rebuilt from hash");
    view_rebuilt_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |view_rebuilt_button| {
        let new_view_rebuilt = view_rebuilt_button.get_active();
        state.borrow_mut().set_view_rebuilt(new_view_rebuilt);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let cropdetect_button = ToggleButton::with_label("cropdetect");
    cropdetect_button.connect_toggled(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label
    => move |cropdetect_button| {
        let new_cropdetect = cropdetect_button.get_active();
        state.borrow_mut().set_cropdetect(new_cropdetect);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let up_button = Button::with_label("up");
    up_button.connect_clicked(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label,

        @strong whole_single_button
    => move |_| {
        state.borrow_mut().decrement_thunk_entry();
        whole_single_button.set_active(true);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let down_button = Button::with_label("down");
    down_button.connect_clicked(clone!(
        @strong state,
        @strong window,
        @strong entries_box,
        @strong idx_label,

        @strong whole_single_button
    => move |_| {
        if whole_single_button.get_active() {
            state.borrow_mut().increment_thunk_entry();
        }
        whole_single_button.set_active(true);
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }));

    let updown_box = Box::new(Vertical, 6);
    updown_box.add(&up_button);
    updown_box.add(&down_button);

    let nav_box = Box::new(Horizontal, 6);
    nav_box.add(&prev_button);
    nav_box.add(&next_button);
    nav_box.add(&updown_box);
    nav_box.add(&whole_single_button);
    nav_box.add(&native_res_button);
    nav_box.add(&cropdetect_button);

    let spa_tempo_box = Box::new(Vertical, 4);
    spa_tempo_box.add(&view_spatial_button);
    spa_tempo_box.add(&view_temporal_button);
    spa_tempo_box.add(&view_rebuilt_button);

    let best_label = Label::new(Some("Best Label"));

    spa_tempo_box.add(&best_label);

    nav_box.add(&spa_tempo_box);

    nav_box.add(&idx_label);

    nav_and_entries.add(&nav_box);
    nav_and_entries.add(&entries_box);

    scroller.add(&nav_and_entries);

    window.add(&scroller);

    //sender2.send(GuiMessage2::Hello).unwrap();

    //keyboard shortcuts!?
    window.connect_key_press_event(clone!(
        @strong window,
        @strong state,
        @strong entries_box,
        @strong idx_label,
        @strong whole_single_button,
        @strong cropdetect_button,
        @strong native_res_button,
        @strong up_button,
        @strong down_button
    => move |window, key| {

        window_connect_key_press_event_callback(
            &window,
            &key,
            &state,
            &entries_box,
            &idx_label,
            &whole_single_button,
            &cropdetect_button,
            &native_res_button,
            &up_button,
            &down_button
        )
    }));

    window.show_all();

    //worker_thread.join().unwrap();
}

#[allow(clippy::too_many_arguments)]
fn window_connect_key_press_event_callback(
    window: &ApplicationWindow,
    key: &gdk::EventKey,

    state: &Rc<RefCell<GuiState>>,
    entries_box: &Box,

    idx_label: &gtk::Label,

    whole_single_button: &ToggleButton,
    cropdetect_button: &ToggleButton,
    native_res_button: &ToggleButton,
    up_button: &Button,
    down_button: &Button,
) -> glib::signal::Inhibit {
    if let Some(c) = key.get_keyval().name() {
        //debug!("Pressed {:?}", c);

        let c = c.as_str().to_lowercase();

        match c.as_str() {
            "right" => {
                state.borrow_mut().next_thunk();
                whole_single_button.set_active(false);
            }
            "left" => {
                state.borrow_mut().prev_thunk();
                whole_single_button.set_active(false);
            }

            "home" => {
                cropdetect_button.set_active(true);
            }

            "end" => {
                cropdetect_button.set_active(false);
            }

            "page_down" => {
                whole_single_button.set_active(true);
            }

            "page_up" => {
                whole_single_button.set_active(false);
            }

            "insert" => {
                native_res_button.set_active(true);
            }

            "delete" => {
                native_res_button.set_active(false);
            }

            "kp_subtract" | "minus" => {
                state.borrow_mut().zoom_out();
                native_res_button.set_active(false);
            }

            "kp_add" | "equal" => {
                state.borrow_mut().zoom_in();
                native_res_button.set_active(false);
            }

            "kp_divide" => {
                state.borrow_mut().set_native(true);
            }

            "kp_multiply" => {
                state.borrow_mut().set_native(false);
            }

            "up" => {
                up_button.clicked();
            }
            "down" => {
                down_button.clicked();
            }

            "comma" => {
                whole_single_button.set_active(false);
                state.borrow_mut().press_key(&c);
            }
            _ => {
                state.borrow_mut().press_key(&c);
            }
        }
        rerender_gui(&state, &entries_box, &window, &idx_label);
    }

    glib::signal::Inhibit(true)
}
